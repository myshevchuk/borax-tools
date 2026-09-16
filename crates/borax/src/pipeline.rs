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
use borax_core::ledger::DuplicateReason;
use borax_core::record::{Record, Source as FieldSource};
use borax_pdf::pure::PurePdf;
use borax_pdf::scan::xmp_title;
use borax_pdf::source::{ExtractionError, PdfSource};
use borax_pdf::tiered::{Extracted, ExtractionConfig, Tier, extract};
use borax_sources::cache::Cache;
use borax_sources::conflict::check_title;
use borax_sources::dispatch::{Unresolved, resolve};
use borax_sources::pace::map_bounded;
use borax_sources::source::{Source, SourceName};
use borax_sources::store::{ContentIndex, hash_file};

use crate::event::{Attempt, Claim, ClaimOrigin, Counts, Event, Overridden, SkipReason};
use crate::ledger::Collection;

/// The files a run works on, as something that can be read.
///
/// The one seam to the filesystem. A run hashes a file before it opens
/// it, because a hash that matches the content index makes opening it
/// unnecessary.
///
/// `Sync` because resolution runs files on a bounded pool of threads
/// ([`borax_sources::pace::map_bounded`]) that share one library.
pub trait Library: Sync {
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
    /// The identifier the run looked up, or `None` when the content
    /// index answered and nothing was looked up at all.
    ///
    /// Kept beside the record because the record's own identifiers are
    /// not evidence about the file: a lookup by arXiv identifier can
    /// return a record carrying a DOI, and only the former was ever
    /// seen in the file.
    pub found: Option<Identifier>,
    /// Every title the file claims for itself, in the order they were
    /// read, and empty when the file was not opened.
    pub claims: Vec<Claim>,
    /// Whether the content index answered, making both extraction and
    /// resolution unnecessary.
    ///
    /// Narrower than "came from a cache": a response cache hit behind a
    /// [`Source`] is invisible from here, so a record served by
    /// [`borax_sources::cache::Cached`] still reports `false`.
    pub cached: bool,
    /// The file's content hash, or `None` when it could not be
    /// computed. Carried because renaming needs it — the planner
    /// recognises an already-named file by content, not by path.
    pub hash: Option<ContentHash>,
    /// The conflict an operator accepted to reach this record, or
    /// `None` when the record cleared the conflict check on its own.
    ///
    /// Only an interactive run can set it: the batch path skips every
    /// conflict it finds, so a record that reaches a caller from there
    /// has nothing to have overridden.
    pub overrode: Option<Overridden>,
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
                // Nothing was looked up and the file was not opened.
                found: None,
                claims: Vec::new(),
                cached: true,
                hash,
                // A record served from the index was accepted by
                // whatever run put it there, not by this one.
                overrode: None,
            });
        }
    }

    let (extracted, claims) = match extract_from(path, library, &config.extraction) {
        Ok(found) => found,
        Err(error) => return FileOutcome::Skipped(skipped_for(&error)),
    };
    let Extracted { identifier, tier } = extracted;
    let looked_up = Identifier::from(identifier);

    let resolved = match resolve(sources, &looked_up) {
        Ok(resolved) => resolved,
        Err(unresolved) => return FileOutcome::Skipped(unresolvable(&unresolved, &looked_up)),
    };

    let claimed: Vec<&str> = claims.iter().map(|claim| claim.title.as_str()).collect();
    if let Some(conflict) = check_title(&claimed, &resolved.record) {
        return FileOutcome::Skipped(SkipReason::Conflict {
            field: conflict.field.to_string(),
            extracted: conflict.extracted,
            resolved: conflict.resolved,
            similarity: conflict.similarity,
        });
    }

    if let Some(hash) = hash.as_ref() {
        index.put(hash, &resolved.record);
    }

    FileOutcome::Resolved(FileRecord {
        record: resolved.record,
        source: Some(resolved.source),
        tier: Some(tier),
        found: Some(looked_up),
        claims,
        cached: false,
        hash,
        // The conflict check has already passed; nothing was overridden.
        overrode: None,
    })
}

/// What a file says about itself, read from the file rather than from
/// a record.
///
/// Available without resolving anything, which is what a caller needs
/// when it is about to compare a record it was handed against the file
/// it is for: a file the content index answered for was never opened,
/// and one no identifier was found in was opened but never asked about
/// its titles.
///
/// An unreadable file claims nothing, which is the same answer as a
/// file carrying no titles: neither is evidence against a record.
pub fn claims_of(path: &Path, library: &dyn Library) -> Vec<Claim> {
    let _ = (path, library);
    todo!("claims_of: the titles the file claims, read on their own")
}

/// Resolve `identifier` for the file at `path`, as a run resolves one
/// it found itself.
///
/// The same services in the same order, and the same title check
/// against the file's own claims — but the check is reported rather
/// than enforced: an identifier a person supplied is a stronger
/// statement than the heuristic that would refuse it, and what the
/// caller does about a disagreement is the caller's to decide.
///
/// Nothing is written to the content index here. A record reached this
/// way is a candidate until somebody accepts it, and [`remember`] is
/// what keeps one that was accepted.
pub fn resolve_supplied(
    path: &Path,
    identifier: &Identifier,
    library: &dyn Library,
    sources: &[&dyn Source],
) -> Result<Supplied, Unresolved> {
    let _ = (path, identifier, library, sources);
    todo!("resolve_supplied: resolve, then report rather than enforce the check")
}

/// A record resolved from an identifier somebody supplied, and what the
/// file has to say about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Supplied {
    pub file: FileRecord,
    /// The disagreement between the record's title and the file's own,
    /// where there is one. Reported, never enforced.
    pub conflict: Option<SkipReason>,
}

/// Keep `record` as what the file at `hash` is, so that no later run
/// asks about it again.
///
/// Best-effort, as every write to the response cache is: an entry that
/// cannot be written leaves the file to be asked about next time,
/// which is the safe way to lose an answer and is never reported as a
/// failure of the rename it followed.
pub fn remember<C: Cache>(index: &ContentIndex<C>, hash: Option<&ContentHash>, record: &Record) {
    let _ = (index, hash, record);
    todo!("remember: write an accepted record to the content index")
}

/// Resolve one file, checking `collection` for it on the way.
///
/// [`resolve_file`] with the ledger's two duplicate checks around it,
/// in the two places the answers become available:
///
/// 1. **Content**: the file's hash is looked up before anything else
///    happens, so a byte-identical re-download is recognised without
///    the file being opened or a single source asked.
/// 2. **Work**: the identifiers of the resolved record are looked up
///    after resolution, which is the earliest a second PDF of an
///    archived paper can be recognised at all.
///
/// A match is [`SkipReason::Duplicate`] carrying the reason and the
/// existing file's full path; the incoming file is left where it is.
/// Only a match whose recorded file is still in the collection counts,
/// so an entry left behind by a deleted or moved file does not keep the
/// incoming one out. Everything else — a skip, an unhashable file, a
/// ledger with no entries — is [`resolve_file`]'s outcome unchanged.
pub fn resolve_file_checking_ledger<C: Cache>(
    path: &Path,
    library: &dyn Library,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    config: &ResolveConfig,
    collection: &Collection<'_>,
) -> FileOutcome {
    if let Some(existing) = library
        .hash(path)
        .ok()
        .and_then(|hash| {
            collection
                .ledger
                .content_duplicate(&hash, &|recorded| collection.counts_against(recorded, path))
        })
        .and_then(|duplicate| collection.live_path(&duplicate))
    {
        return FileOutcome::Skipped(SkipReason::Duplicate {
            reason: DuplicateReason::Content,
            existing_path: existing,
        });
    }

    let outcome = resolve_file(path, library, sources, index, config);

    let FileOutcome::Resolved(file) = &outcome else {
        return outcome;
    };
    match collection
        .ledger
        .work_duplicate(&identifiers_of(&file.record), &|recorded| {
            collection.counts_against(recorded, path)
        })
        .and_then(|duplicate| collection.live_path(&duplicate))
    {
        Some(existing) => FileOutcome::Skipped(SkipReason::Duplicate {
            reason: DuplicateReason::Work,
            existing_path: existing,
        }),
        None => outcome,
    }
}

/// Every identifier `record` carries, in the order DOI, arXiv, PMID,
/// ISBN — the same order [`borax_core::ledger::Entry::identifiers`]
/// uses, so the ledger is asked about an incoming file's identifiers in
/// the order it recorded an admitted one's.
fn identifiers_of(record: &Record) -> Vec<Identifier> {
    let mut identifiers = Vec::new();
    if let Some(doi) = &record.doi {
        identifiers.push(Identifier::Doi(doi.clone()));
    }
    if let Some(arxiv) = &record.borax.arxiv {
        identifiers.push(Identifier::Arxiv(arxiv.clone()));
    }
    if let Some(pmid) = &record.pmid {
        identifiers.push(Identifier::Pmid(*pmid));
    }
    if let Some(isbn) = &record.isbn {
        identifiers.push(Identifier::Isbn(isbn.clone()));
    }
    identifiers
}

/// Extract from the file at `path`, along with every title its own
/// metadata claims.
///
/// A document may hold two — the XMP packet's `dc:title` and the Info
/// dictionary's — and they need not agree with each other or with
/// anything. Both are collected rather than one preferred, because
/// which of them is worth believing cannot be known here: a publisher
/// PDF whose XMP holds a producer's placeholder may carry the real
/// title in its Info dictionary, and the reverse happens just as often.
/// [`borax_sources::conflict::check_title`] is what decides which are
/// evidence.
///
/// The titles are taken while the document is open, since the conflict
/// check runs after it has been dropped.
fn extract_from(
    path: &Path,
    library: &dyn Library,
    config: &ExtractionConfig,
) -> Result<(Extracted, Vec<Claim>), ExtractionError> {
    let pdf = library.open(path)?;
    let extracted = extract(pdf.as_ref(), config)?;
    let claimed = [
        (ClaimOrigin::Xmp, pdf.xmp().and_then(xmp_title)),
        (ClaimOrigin::Info, pdf.info_metadata().title.clone()),
    ];
    Ok((
        extracted,
        claimed
            .into_iter()
            .filter_map(|(from, title)| {
                Some(Claim {
                    from,
                    title: title?,
                })
            })
            .collect(),
    ))
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

/// The skip a failed resolution reports, naming the identifier that
/// was looked up and keeping the attempts in the order the sources
/// were asked.
fn unresolvable(unresolved: &Unresolved, found: &Identifier) -> SkipReason {
    SkipReason::Unresolvable {
        found: found.to_string(),
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

/// The services that supplied `file`'s record, as the event names them.
///
/// The service the resolver used, where the run looked one up. Where
/// the content index answered instead, the sources the record's own
/// per-field provenance names, other than extraction itself: a record
/// is the work of whoever's fields it carries, and the index is only
/// where it was kept. They are named in one fixed order — Crossref,
/// OpenAlex, arXiv, DataCite, PubMed, then a sidecar — rather than in
/// [`borax_sources::dispatch::priority`]'s, which differs by identifier
/// type and omits services a record can still carry a field from.
///
/// A record whose provenance names no such source reports the index
/// itself, as `cache`, since nothing else is known about where it came
/// from.
fn sources_of(file: &FileRecord) -> String {
    if let Some(source) = file.source {
        return source.as_str().to_string();
    }

    // The order a record's makers are named in, fixed rather than
    // taken from the resolver's priority, which differs by identifier
    // type and leaves out services a record can carry a field from.
    const ORDER: [(FieldSource, &str); 6] = [
        (FieldSource::Crossref, "crossref"),
        (FieldSource::OpenAlex, "openalex"),
        (FieldSource::Arxiv, "arxiv"),
        (FieldSource::DataCite, "datacite"),
        (FieldSource::PubMed, "pubmed"),
        (FieldSource::Sidecar, "sidecar"),
    ];
    let named: Vec<&str> = ORDER
        .iter()
        .filter(|(source, _)| {
            file.record
                .borax
                .provenance
                .values()
                .any(|had| had == source)
        })
        .map(|(_, name)| *name)
        .collect();

    match named.is_empty() {
        true => "cache".to_string(),
        false => named.join(", "),
    }
}

/// The event that reports `outcome` for `path`.
///
/// A resolved file whose source is unknown — the content index
/// answered — reports its source as `cache`.
pub fn event_for(path: &Path, outcome: &FileOutcome) -> Event {
    match outcome {
        FileOutcome::Resolved(file) => resolved_event(path, file),
        FileOutcome::Skipped(reason) => Event::Skipped {
            path: path.to_path_buf(),
            reason: reason.clone(),
        },
    }
}

/// The event that reports `file` as `path`'s resolution.
///
/// Public because the description an interactive run shows is a
/// rendering of this event and of nothing else: the operator at the
/// terminal and a reader of the stream are told the same things about a
/// file, and the way to keep that true is for both to be made from the
/// same value.
pub fn resolved_event(path: &Path, file: &FileRecord) -> Event {
    Event::Resolved {
        path: path.to_path_buf(),
        identifier: identifier_of(&file.record),
        record: Box::new(file.record.clone()),
        source: sources_of(file),
        found: file
            .found
            .as_ref()
            .map_or_else(|| identifier_of(&file.record), Identifier::to_string),
        claims: file.claims.clone(),
        tier: file.tier.map(|tier| tier.as_str().to_string()),
        overrode: file.overrode.clone(),
        cached: file.cached,
    }
}

/// The identifier a record is reported under: its DOI, else its arXiv
/// id, else its PMID, else its ISBN, and empty when the record carries
/// none.
///
/// Rendered through [`Identifier`]'s own `Display`, so the value
/// carries its kind (`doi:10.1000/xyz`) rather than leaving a consumer
/// to guess which of four kinds a bare string is.
fn identifier_of(record: &Record) -> String {
    record
        .doi
        .as_ref()
        .map(|id| Identifier::Doi(id.clone()))
        .or_else(|| {
            record
                .borax
                .arxiv
                .as_ref()
                .map(|id| Identifier::Arxiv(id.clone()))
        })
        .or_else(|| record.pmid.as_ref().map(|id| Identifier::Pmid(*id)))
        .or_else(|| record.isbn.as_ref().map(|id| Identifier::Isbn(id.clone())))
        .map(|identifier| identifier.to_string())
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
/// `config` is asked per file rather than fixed for the batch, because
/// the settings a file runs under come from its own directory: one
/// invocation can span two trees that configure extraction differently.
///
/// Up to `concurrency` files are resolved at once
/// ([`borax_sources::pace::map_bounded`]). Ordering is unaffected: that
/// helper restores input order whatever order the jobs finish in, so
/// the event stream is the same as a sequential run\'s and a run
/// remains diffable against itself. Pacing is what keeps concurrency
/// from becoming rudeness — it lives in the sources, one interval per
/// service, so however many threads are running no service is asked
/// faster than it allows.
pub fn resolve_batch<C: Cache>(
    paths: &[PathBuf],
    library: &dyn Library,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    config: &(dyn Fn(&Path) -> ResolveConfig + Sync),
    concurrency: usize,
) -> Run {
    let outcomes = map_bounded(paths.to_vec(), concurrency, |path| {
        let outcome = resolve_file(&path, library, sources, index, &config(&path));
        (path, outcome)
    });

    let mut events = Vec::with_capacity(outcomes.len() + 1);
    let mut counts = Counts::default();
    for (path, outcome) in &outcomes {
        match outcome {
            FileOutcome::Resolved(_) => counts.resolved += 1,
            FileOutcome::Skipped(_) => counts.skipped += 1,
        }
        events.push(event_for(path, outcome));
    }

    events.push(Event::RunFinished { counts });
    Run { events, counts }
}

/// A [`Library`] backed by the real filesystem and the pure-Rust PDF
/// engine.
#[derive(Debug, Clone, Copy)]
pub struct RealLibrary;

impl Library for RealLibrary {
    /// The file's content hash, read in chunks so a large PDF is never
    /// held in memory whole.
    ///
    /// A file that cannot be read reports
    /// [`ExtractionError::Unreadable`] carrying the I/O error, which is
    /// what [`resolve_file`] treats as a content-index miss rather than
    /// as a verdict.
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        hash_file(path).map_err(|error| ExtractionError::Unreadable {
            message: error.to_string(),
        })
    }

    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        Ok(Box::new(PurePdf::open(path)?))
    }
}
