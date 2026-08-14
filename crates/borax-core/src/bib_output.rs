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
    let _ = (existing, additions, policy);
    todo!("merge additions into the master file")
}

/// Marker prefixing the JSON line in a sidecar file.
pub const SIDECAR_MARKER: &str = "% borax-record: ";

/// Render a per-file sidecar: the BibTeX entry, one blank line, then
/// the full canonical record as single-line JSON prefixed by
/// [`SIDECAR_MARKER`], with a trailing newline. BibTeX tooling reads
/// the entry (text outside entries is ignored); borax recovers the
/// lossless record with [`parse_sidecar_record`].
pub fn sidecar(record: &Record, key: &str) -> String {
    let _ = (record, key);
    todo!("render the sidecar")
}

/// Recover the canonical record from sidecar content: the first line
/// starting with [`SIDECAR_MARKER`] whose remainder parses as a
/// record. `None` when no such line exists.
pub fn parse_sidecar_record(content: &str) -> Option<Record> {
    let _ = content;
    todo!("parse the sidecar's JSON line")
}
