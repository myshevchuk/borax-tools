//! Turning a file into a record: extraction, resolution, and the
//! decision to leave it alone.
//!
//! This is where the four crates meet. Everything they contribute is
//! already tested in isolation, so what is tested here is the
//! composition: which order the passes run in, what short-circuits
//! them, and which outcome each failure produces.
//!
//! The filesystem enters through [`Library`] alone. Every other input —
//! the sources, the cache, the extraction limits — is already a trait
//! or a value, so a whole batch runs in a test with no disk and no
//! network.

use std::path::{Path, PathBuf};

use borax_core::content::ContentHash;
use borax_core::identifier::Identifier;
use borax_core::record::Record;
use borax_pdf::source::{ExtractionError, PdfSource};
use borax_pdf::tiered::{Extracted, ExtractionConfig, Tier, extract};
use borax_sources::cache::Cache;
use borax_sources::conflict::check_title;
use borax_sources::dispatch::{Unresolved, resolve};
use borax_sources::source::{Source, SourceName};
use borax_sources::store::ContentIndex;

use crate::event::{Attempt, Counts, Event, SkipReason};

/// The files a run works on, as something that can be read.
///
/// The one seam to the filesystem. A run hashes a file before it opens
/// it, because a hash that matches the content index makes opening it
/// unnecessary.
pub trait Library {
    /// The content hash of the file at `path`.
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError>;

    /// Open `path` as a PDF.
    ///
    /// Reports [`ExtractionError::Unreadable`] or
    /// [`ExtractionError::Encrypted`]; failing to find an identifier is
    /// not this method's business.
    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError>;
}

/// A record, and how the run came by it.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRecord {
    pub record: Record,
    /// Which service supplied it, or `None` when the content index
    /// answered and the service is no longer known.
    pub source: Option<SourceName>,
    /// Which extraction pass found the identifier, or `None` when
    /// extraction never ran because the content index answered.
    pub tier: Option<Tier>,
    /// Whether the answer came from a cache rather than the network.
    pub cached: bool,
}

/// What a run decided about one file.
///
// One of these is produced per file and moved once, so the 352 bytes
// clippy objects to are moved once per file too. Boxing would put an
// allocation on the successful path — the common one — to shrink a
// value nothing keeps.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum FileOutcome {
    Resolved(FileRecord),
    Skipped(SkipReason),
}

/// What a run may do while resolving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveConfig {
    /// How far the text pass reads.
    pub extraction: ExtractionConfig,
    /// Whether the caches may be read and written. `false` is the
    /// `--no-cache` bypass: every file is extracted and every
    /// identifier asked about again.
    pub cache: bool,
}

/// Resolve one file.
///
/// The passes, in order, each ending the run when it succeeds:
///
/// 1. **Content index**: the file's hash is looked up, and a hit is
///    returned without opening the file at all. Skipped entirely when
///    [`ResolveConfig::cache`] is `false`, and treated as a miss when
///    the file cannot be hashed — a hash failure is not yet a reason to
///    give up, since opening the file reports a better one.
/// 2. **Extraction**: [`borax_pdf::tiered::extract`] over the opened
///    file.
/// 3. **Resolution**: [`borax_sources::dispatch::resolve`] over
///    `sources`, which are consulted in priority order for the
///    identifier's type.
/// 4. **Conflict check**: the file's own title, when it has one, is
///    compared against the resolved record's
///    ([`borax_sources::conflict::check_title`]). A disagreement is a
///    skip, not a result: a record for the wrong work is worse than no
///    record.
///
/// A successful resolution is written to the content index under the
/// file's hash, so a later run recognises the file under any name.
/// The write happens even when [`ResolveConfig::cache`] is `false`:
/// the bypass forces a live answer, and the point of forcing one is
/// usually that the stored answer was wrong, so the fresh record
/// replaces it. A run that must leave the cache untouched clears it
/// instead.
///
/// Extraction failures map onto skip reasons as follows:
/// [`ExtractionError::Unreadable`] and [`ExtractionError::Encrypted`]
/// both become [`SkipReason::Unreadable`] carrying the error's own
/// message, which keeps an encrypted file distinguishable from a
/// corrupt one; [`ExtractionError::NoTextLayer`] and
/// [`ExtractionError::NoIdentifierFound`] both become
/// [`SkipReason::NoIdentifier`], since to a user both mean the file
/// said nothing about what it is.
///
/// Never panics and never propagates an error: every failure is a
/// [`FileOutcome::Skipped`] carrying the reason, because one unreadable
/// file must not end a batch.
pub fn resolve_file<C: Cache>(
    path: &Path,
    library: &dyn Library,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    config: &ResolveConfig,
) -> FileOutcome {
    let hash = library.hash(path).ok();

    if config.cache {
        if let Some(record) = hash.as_ref().and_then(|hash| index.get(hash)) {
            return FileOutcome::Resolved(FileRecord {
                record,
                source: None,
                tier: None,
                cached: true,
            });
        }
    }

    let (extracted, extracted_title) = match extract_from(path, library, &config.extraction) {
        Ok(found) => found,
        Err(error) => return FileOutcome::Skipped(skipped_for(&error)),
    };
    let Extracted { identifier, tier } = extracted;

    let resolved = match resolve(sources, &Identifier::from(identifier)) {
        Ok(resolved) => resolved,
        Err(unresolved) => return FileOutcome::Skipped(unresolvable(&unresolved)),
    };

    if let Some(conflict) = check_title(extracted_title.as_deref(), &resolved.record) {
        return FileOutcome::Skipped(SkipReason::Conflict {
            field: conflict.field.to_string(),
            extracted: conflict.extracted,
            resolved: conflict.resolved,
        });
    }

    if let Some(hash) = hash.as_ref() {
        index.put(hash, &resolved.record);
    }

    FileOutcome::Resolved(FileRecord {
        record: resolved.record,
        source: Some(resolved.source),
        tier: Some(tier),
        cached: false,
    })
}

/// Extract from the file at `path`, along with the title its own
/// metadata claims.
///
/// The title is taken while the document is open, since the conflict
/// check runs after it has been dropped.
fn extract_from(
    path: &Path,
    library: &dyn Library,
    config: &ExtractionConfig,
) -> Result<(Extracted, Option<String>), ExtractionError> {
    let pdf = library.open(path)?;
    let extracted = extract(pdf.as_ref(), config)?;
    Ok((extracted, pdf.info_metadata().title.clone()))
}

/// The skip an extraction failure reports.
fn skipped_for(error: &ExtractionError) -> SkipReason {
    match error {
        ExtractionError::Unreadable { message } => SkipReason::Unreadable {
            message: message.clone(),
        },
        ExtractionError::Encrypted => SkipReason::Unreadable {
            message: error.to_string(),
        },
        ExtractionError::NoTextLayer | ExtractionError::NoIdentifierFound => {
            SkipReason::NoIdentifier
        }
    }
}

/// The skip a failed resolution reports, keeping the attempts in the
/// order the sources were asked.
fn unresolvable(unresolved: &Unresolved) -> SkipReason {
    SkipReason::Unresolvable {
        attempts: unresolved
            .attempts
            .iter()
            .map(|(source, error)| Attempt {
                source: source.to_string(),
                error: error.to_string(),
            })
            .collect(),
    }
}

/// The event that reports `outcome` for `path`.
///
/// A resolved file whose source is unknown — the content index
/// answered — reports its source as `cache`.
pub fn event_for(path: &Path, outcome: &FileOutcome) -> Event {
    match outcome {
        FileOutcome::Resolved(file) => Event::Resolved {
            path: path.to_path_buf(),
            identifier: identifier_of(&file.record),
            source: file
                .source
                .map_or("cache", |source| source.as_str())
                .to_string(),
            tier: file.tier.map(|tier| tier.as_str().to_string()),
            cached: file.cached,
        },
        FileOutcome::Skipped(reason) => Event::Skipped {
            path: path.to_path_buf(),
            reason: reason.clone(),
        },
    }
}

/// The identifier a record is reported under: its DOI, else its arXiv
/// id, else its PMID, else its ISBN, each in the normalized form its own
/// type renders, and empty when the record carries none.
fn identifier_of(record: &Record) -> String {
    record
        .doi
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| record.borax.arxiv.as_ref().map(ToString::to_string))
        .or_else(|| record.pmid.as_ref().map(ToString::to_string))
        .or_else(|| record.isbn.as_ref().map(ToString::to_string))
        .unwrap_or_default()
}

/// Everything a run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// The events in the order they were produced, ending with
    /// [`Event::RunFinished`].
    pub events: Vec<Event>,
    /// The totals carried by that final event.
    pub counts: Counts,
}

/// Resolve every file in `paths`.
///
/// Files are processed in the order given and each contributes exactly
/// one event, so the stream is deterministic: the same inputs produce
/// the same lines, which is what makes `--json` output diffable between
/// runs.
///
/// The run is sequential. Bounded concurrency
/// ([`borax_sources::pace::map_bounded`]) belongs here but arrives with
/// the real transport, and because that helper restores input order it
/// can be introduced without changing a single event.
pub fn resolve_batch<C: Cache>(
    paths: &[PathBuf],
    library: &dyn Library,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    config: &ResolveConfig,
) -> Run {
    let mut events = Vec::with_capacity(paths.len() + 1);
    let mut counts = Counts::default();

    for path in paths {
        let outcome = resolve_file(path, library, sources, index, config);
        match outcome {
            FileOutcome::Resolved(_) => counts.resolved += 1,
            FileOutcome::Skipped(_) => counts.skipped += 1,
        }
        events.push(event_for(path, &outcome));
    }

    events.push(Event::RunFinished { counts });
    Run { events, counts }
}
