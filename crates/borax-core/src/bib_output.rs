//! Bibliography output: master-`.bib` merging and per-file sidecars.
//!
//! The merge treats the existing file as opaque bytes plus a tolerant
//! scan: pre-existing content is never reformatted, reordered, or
//! re-wrapped — additions append at the end, and an update splices
//! exactly one entry's span. Deduplication keys on identifiers (DOI,
//! then arXiv id), never on citation keys.
//!
//! The scanner recognizes entries of the form `@type{key,` (letters in
//! `type`, any spacing), finds each entry's extent by brace counting,
//! and reads `doi` / `eprint` fields named case-insensitively with
//! brace- or quote-delimited values. `@comment`, `@string`, and
//! `@preamble` blocks are ignored. Text outside entries is preserved
//! untouched (BibTeX ignores it).

use std::collections::{BTreeMap, BTreeSet};

use crate::bibtex::emit;
use crate::identifier::{ArxivId, Doi};
use crate::record::Record;

/// What to do when an addition's identifier already has an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy {
    /// Leave the existing entry; report `AlreadyPresent`.
    Skip,
    /// Replace the existing entry in place, keeping its citation key
    /// (citations in documents stay valid).
    Update,
}

/// What happened to one addition, parallel to the input slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Appended under `key` (the requested key, possibly suffixed for
    /// uniqueness).
    Added { key: String },
    /// An entry with the same identifier exists; nothing changed.
    AlreadyPresent { existing_key: String },
    /// The existing entry was replaced in place under its own key.
    Updated { key: String },
}

/// A merge's result: the new file content and one outcome per
/// addition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult {
    pub content: String,
    pub outcomes: Vec<MergeOutcome>,
}

/// Merge `additions` into the content of a master `.bib` file.
///
/// Each addition is a desired citation key (from the template engine)
/// plus the record to emit. Per addition, in order:
///
/// 1. **Dedup**: the addition's identity is its DOI, else its arXiv
///    id, compared in normalized form against the existing entries'
///    `doi`/`eprint` fields (and against entries added earlier in this
///    merge). On a match: `Skip` → `AlreadyPresent`, `Update` →
///    re-emit via [`crate::bibtex::emit`] under the matched entry's
///    key and splice over exactly that entry's span. An addition with
///    neither identifier is never a duplicate.
/// 2. **Key uniqueness**: the requested key, if taken by any existing
///    or earlier-added entry (byte equality), gets the first free
///    letter suffix appended: `smith2024` → `smith2024a`, `b` … `z`,
///    `aa`, ….
/// 3. **Append**: the entry (from [`crate::bibtex::emit`]) goes to
///    the end of the file, separated from what precedes it by exactly
///    one blank line; a missing final newline on the existing content
///    is added first. Pre-existing bytes are otherwise unchanged.
///
/// Deterministic: same inputs, same result.
pub fn merge(
    existing: &str,
    additions: &[(&str, &Record)],
    policy: DuplicatePolicy,
) -> MergeResult {
    let entries = scan(existing);

    let mut keys: BTreeSet<String> = entries.iter().map(|entry| entry.key.clone()).collect();
    let mut by_identity: BTreeMap<String, Target> = BTreeMap::new();
    for entry in &entries {
        for identity in entry.identities() {
            by_identity.entry(identity).or_insert_with(|| Target {
                key: entry.key.clone(),
                location: Location::Existing {
                    start: entry.start,
                    end: entry.end,
                },
            });
        }
    }

    let mut splices: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    let mut appended: Vec<String> = Vec::new();
    let mut outcomes = Vec::with_capacity(additions.len());

    for (requested, record) in additions {
        let identity = record_identity(record);
        let matched = identity
            .as_ref()
            .and_then(|identity| by_identity.get(identity))
            .map(|target| (target.key.clone(), target.location));

        if let Some((key, location)) = matched {
            outcomes.push(match policy {
                DuplicatePolicy::Skip => MergeOutcome::AlreadyPresent { existing_key: key },
                DuplicatePolicy::Update => {
                    let replacement = emit(record, &key);
                    match location {
                        Location::Existing { start, end } => {
                            splices.insert(start, (end, replacement));
                        }
                        Location::Pending(index) => appended[index] = replacement,
                    }
                    MergeOutcome::Updated { key }
                }
            });
            continue;
        }

        let key = unique_key(requested, &keys);
        keys.insert(key.clone());
        appended.push(emit(record, &key));
        if let Some(identity) = identity {
            by_identity.insert(
                identity,
                Target {
                    key: key.clone(),
                    location: Location::Pending(appended.len() - 1),
                },
            );
        }
        outcomes.push(MergeOutcome::Added { key });
    }

    MergeResult {
        content: assemble(existing, &splices, &appended),
        outcomes,
    }
}

/// Splice the replacements over their spans, then append the new
/// entries, each separated from what precedes it by one blank line.
fn assemble(
    existing: &str,
    splices: &BTreeMap<usize, (usize, String)>,
    appended: &[String],
) -> String {
    let mut content = String::with_capacity(existing.len());
    let mut cursor = 0;
    for (start, (end, replacement)) in splices {
        content.push_str(&existing[cursor..*start]);
        content.push_str(replacement);
        cursor = *end;
    }
    content.push_str(&existing[cursor..]);

    if appended.is_empty() {
        return content;
    }
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    for entry in appended {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(entry);
    }
    content
}

/// `requested` itself when free, else the first free letter-suffixed
/// form of it.
fn unique_key(requested: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(requested) {
        return requested.to_string();
    }
    let mut index = 0;
    loop {
        let candidate = format!("{requested}{}", letter_suffix(index));
        if !taken.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

/// The `index`-th collision suffix: `a`, `b`, … `z`, `aa`, `ab`, … —
/// the ladder both the merge's keys and [`crate::rename`]'s targets
/// climb.
pub(crate) fn letter_suffix(index: usize) -> String {
    let mut letters = Vec::new();
    let mut remaining = index;
    loop {
        letters.push(char::from(b'a' + (remaining % 26) as u8));
        if remaining < 26 {
            break;
        }
        remaining = remaining / 26 - 1;
    }
    letters.iter().rev().collect()
}

/// Where the entry a duplicate matched lives: in the pre-existing bytes,
/// or among the entries this merge is about to append.
#[derive(Debug, Clone, Copy)]
enum Location {
    Existing { start: usize, end: usize },
    Pending(usize),
}

struct Target {
    key: String,
    location: Location,
}

/// The identity an addition dedups on: its DOI, else its arXiv id.
/// Namespaced so the two kinds never collide.
fn record_identity(record: &Record) -> Option<String> {
    match (&record.doi, &record.borax.arxiv) {
        (Some(doi), _) => Some(doi_identity(doi)),
        (None, Some(arxiv)) => Some(arxiv_identity(arxiv)),
        (None, None) => None,
    }
}

fn doi_identity(doi: &Doi) -> String {
    format!("doi:{}", doi.as_str())
}

fn arxiv_identity(arxiv: &ArxivId) -> String {
    format!("arxiv:{}", arxiv.id())
}

/// One entry the scanner found: its citation key, its byte span, and the
/// identifiers its fields carry. The span runs from the `@` through the
/// newline ending the closing brace's line, so splicing over it leaves
/// the surrounding blank lines as they were.
struct ScannedEntry {
    key: String,
    start: usize,
    end: usize,
    doi: Option<Doi>,
    arxiv: Option<ArxivId>,
}

impl ScannedEntry {
    /// Every identity the entry answers to: an entry carrying both a DOI
    /// and an arXiv id matches an addition keyed on either.
    fn identities(&self) -> Vec<String> {
        let mut identities = Vec::new();
        if let Some(doi) = &self.doi {
            identities.push(doi_identity(doi));
        }
        if let Some(arxiv) = &self.arxiv {
            identities.push(arxiv_identity(arxiv));
        }
        identities
    }
}

/// Block types that carry no bibliographic entry; their extent is still
/// brace-matched so their contents are never read as one.
const IGNORED_BLOCKS: [&str; 3] = ["comment", "string", "preamble"];

/// Find the file's entries, in file order.
fn scan(content: &str) -> Vec<ScannedEntry> {
    let bytes = content.as_bytes();
    let mut entries = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = bytes[cursor..].iter().position(|byte| *byte == b'@') {
        let start = cursor + offset;
        let mut index = start + 1;
        let kind_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let kind = &content[kind_start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if kind.is_empty() || bytes.get(index) != Some(&b'{') {
            cursor = start + 1;
            continue;
        }

        // An unbalanced brace makes every later offset a guess, so the
        // scan stops rather than reporting spans it cannot trust.
        let Some(close) = matching_brace(bytes, index) else {
            break;
        };
        cursor = close + 1;
        if IGNORED_BLOCKS
            .iter()
            .any(|block| kind.eq_ignore_ascii_case(block))
        {
            continue;
        }

        let body = &content[index + 1..close];
        let (key, fields) = match body.find(',') {
            Some(comma) => (&body[..comma], &body[comma + 1..]),
            None => (body, ""),
        };
        let (doi, arxiv) = scan_fields(fields);
        entries.push(ScannedEntry {
            key: key.trim().to_string(),
            start,
            end: if bytes.get(close + 1) == Some(&b'\n') {
                close + 2
            } else {
                close + 1
            },
            doi,
            arxiv,
        });
    }

    entries
}

/// The index of the `}` closing the `{` at `open`; `None` when the
/// braces never balance.
fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Read the `doi` and `eprint` fields out of an entry body (everything
/// after the citation key's comma), taking the first of each.
fn scan_fields(body: &str) -> (Option<Doi>, Option<ArxivId>) {
    let bytes = body.as_bytes();
    let mut doi = None;
    let mut arxiv = None;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() || bytes[index] == b',' {
            index += 1;
            continue;
        }

        let name_start = index;
        while index < bytes.len() && is_field_name_byte(bytes[index]) {
            index += 1;
        }
        if index == name_start {
            index += 1;
            continue;
        }
        let name = &body[name_start..index];

        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index) != Some(&b'=') {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }

        let (value, next) = read_value(body, index);
        index = next;
        if doi.is_none() && name.eq_ignore_ascii_case("doi") {
            doi = Doi::parse(value).ok();
        } else if arxiv.is_none() && name.eq_ignore_ascii_case("eprint") {
            arxiv = ArxivId::parse(value).ok();
        }
    }

    (doi, arxiv)
}

fn is_field_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

/// The field value starting at `index` — `{braced}`, `"quoted"`, or a
/// bare token — and the offset just past it.
fn read_value(body: &str, index: usize) -> (&str, usize) {
    let bytes = body.as_bytes();
    match bytes.get(index) {
        Some(b'{') => match matching_brace(bytes, index) {
            Some(close) => (&body[index + 1..close], close + 1),
            None => (&body[index + 1..], bytes.len()),
        },
        Some(b'"') => {
            let rest = &body[index + 1..];
            match rest.find('"') {
                Some(close) => (&rest[..close], index + 2 + close),
                None => (rest, bytes.len()),
            }
        }
        _ => {
            let end = body[index..]
                .find(',')
                .map_or(bytes.len(), |comma| index + comma);
            (body[index..end].trim(), end)
        }
    }
}

/// Marker prefixing the JSON line in a sidecar file.
pub const SIDECAR_MARKER: &str = "% borax-record: ";

/// Render a per-file sidecar: the BibTeX entry, one blank line, then
/// the full canonical record as single-line JSON prefixed by
/// [`SIDECAR_MARKER`], with a trailing newline. BibTeX tooling reads
/// the entry (text outside entries is ignored); borax recovers the
/// lossless record with [`parse_sidecar_record`].
pub fn sidecar(record: &Record, key: &str) -> String {
    // Serialization fails only on a non-finite confidence; an empty
    // payload degrades the sidecar instead of panicking.
    let json = serde_json::to_string(record).unwrap_or_default();
    format!("{}\n{SIDECAR_MARKER}{json}\n", emit(record, key))
}

/// Recover the canonical record from sidecar content: the first line
/// starting with [`SIDECAR_MARKER`] whose remainder parses as a
/// record. `None` when no such line exists.
pub fn parse_sidecar_record(content: &str) -> Option<Record> {
    content
        .lines()
        .filter_map(|line| line.strip_prefix(SIDECAR_MARKER))
        .find_map(|json| serde_json::from_str(json).ok())
}
