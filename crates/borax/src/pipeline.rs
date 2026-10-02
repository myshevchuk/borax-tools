//! Turning a file into a record: extraction, resolution, and the
//! decision to leave it alone.
//!
//! This is where the four crates meet. Everything they contribute is
//! already tested in isolation, so what is tested here is the
//! composition: which order the passes run in, what short-circuits
//! them, and which outcome each failure produces.
//!
//! The filesystem enters through [`Documents`] alone. Every other input —
//! the sources, the cache, the extraction limits — is already a trait
//! or a value, so a whole batch runs in a test with no disk and no
//! network.

use std::path::{Path, PathBuf};

use borax_core::content::ContentHash;
use borax_core::identifier::Identifier;
use borax_core::library::DuplicateReason;
use borax_core::record::{Record, Source as FieldSource};
use borax_pdf::pure::PurePdf;
use borax_pdf::scan::xmp_title;
use borax_pdf::source::{ExtractionError, PdfSource};
use borax_pdf::tiered::{Extracted, ExtractionConfig, Tier, extract};
use borax_sources::cache::Cache;
use borax_sources::conflict::{TitleCheck, check_title};
use borax_sources::dispatch::{Resolved, Unresolved, resolve};
use borax_sources::pace::map_bounded;
use borax_sources::source::{Source, SourceName};
use borax_sources::store::{ContentIndex, hash_file};

use crate::event::{
    Attempt, Claim, ClaimOrigin, Counts, Event, Extraction, LibraryAnswer, Overridden, SkipReason,
};
use crate::evidence::{
    Consultation, Evidence, ExtractionEvidence, ExtractionStep, IndexEvidence, IndexRead,
    IndexWrite, LookupEvidence, MatchCheck, Origin, RecordRetrieval, ServiceAttempt, Titles,
    Unattempted,
};
use crate::library::{Account, Consulted, Stores, WorkDuplicate};

/// The documents a run works on, as something that can be read.
///
/// Not the library a run writes to: this is the seam file bytes are
/// read through, and [`crate::library`] is the tree they sit in.
///
/// The one seam to the filesystem. A run hashes a file before it opens
/// it, because a hash its library tracks, or one that matches the
/// content index, makes opening it unnecessary.
///
/// `Sync` because resolution runs files on a bounded pool of threads
/// ([`borax_sources::pace::map_bounded`]) that share one reader.
pub trait Documents: Sync {
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
    /// The file's content hash, or `None` when it could not be
    /// computed. Carried because renaming needs it — the planner
    /// recognises an already-named file by content, not by path.
    pub hash: Option<ContentHash>,
    /// What the resolution that reached `record` found out about the
    /// file, step by step. The methods below derive every other fact
    /// about the record's provenance from it.
    pub evidence: Evidence,
    /// The conflict an operator accepted to reach this record, or
    /// `None` when the record cleared the conflict check on its own.
    ///
    /// Only an interactive run can set it: the batch path skips every
    /// conflict it finds, so a record that reaches a caller from there
    /// has nothing to have overridden.
    pub overrode: Option<Overridden>,
}

impl FileRecord {
    /// The service that supplied the record, or `None` when no service
    /// did: the content index or the library answered.
    ///
    /// Read from [`Evidence::retrieval`], so a record a service served
    /// from its response cache names that service.
    pub fn source(&self) -> Option<SourceName> {
        match self.evidence.retrieval()? {
            RecordRetrieval::ServiceCache { service } | RecordRetrieval::Network { service } => {
                Some(service)
            }
            RecordRetrieval::Library { .. } | RecordRetrieval::ContentIndex => None,
        }
    }

    /// Where the identifier the record was reached by came from, or
    /// `None` when the content index answered and nothing was looked
    /// up.
    ///
    /// Read from [`Evidence::retrieval`]: a record a service supplied
    /// gives the extraction pass that read the identifier, or
    /// [`Provenance::Supplied`] for an operator's; a record the library
    /// supplied gives [`Provenance::Library`]. What the library said
    /// about the file makes no difference to a record a service
    /// supplied.
    pub fn tier(&self) -> Option<Provenance> {
        match self.evidence.retrieval()? {
            RecordRetrieval::ServiceCache { .. } | RecordRetrieval::Network { .. } => {
                match &self.evidence.lookup {
                    LookupEvidence::Attempted {
                        origin: Origin::Extracted(tier),
                        ..
                    } => Some(Provenance::Extracted(*tier)),
                    LookupEvidence::Attempted {
                        origin: Origin::Operator,
                        ..
                    } => Some(Provenance::Supplied),
                    LookupEvidence::NotAttempted(_) => None,
                }
            }
            RecordRetrieval::Library { .. } => Some(Provenance::Library),
            RecordRetrieval::ContentIndex => None,
        }
    }

    /// The identifier the run looked up, or `None` when nothing was
    /// looked up because the content index or the library answered.
    ///
    /// Kept apart from the record because the record's own identifiers
    /// are not evidence about the file: a lookup by arXiv identifier can
    /// return a record carrying a DOI, and only the former was ever
    /// seen in the file.
    pub fn found(&self) -> Option<&Identifier> {
        match &self.evidence.lookup {
            LookupEvidence::Attempted { identifier, .. } => Some(identifier),
            LookupEvidence::NotAttempted(_) => None,
        }
    }

    /// Every title the file claims for itself, in the order they were
    /// read, and none when the file was not opened or could not be.
    pub fn claims(&self) -> &[Claim] {
        self.evidence.extraction.titles.claims()
    }

    /// Whether the content index answered, making both extraction and
    /// resolution unnecessary. `false` for a library answer and for a
    /// service's response cache, neither of which is the content index.
    pub fn cached(&self) -> bool {
        self.evidence.retrieval() == Some(RecordRetrieval::ContentIndex)
    }

    /// What the run's library said about the file, or `None` when it
    /// was not asked.
    ///
    /// [`LibraryAnswer::Tracked`] with [`FileRecord::tier`]
    /// [`Provenance::Library`] is a record the library supplied; with
    /// any other tier, the library answered and the record was reached
    /// some other way, as when an operator re-identified the file. A
    /// problem answer is a record reached by the passes the library
    /// could not spare.
    pub fn library(&self) -> Option<&LibraryAnswer> {
        self.evidence.library.answer()
    }

    /// The disagreement between the record's title and the file's own,
    /// as the [`SkipReason::Conflict`] a batch run skips on, or `None`
    /// when the title check did not conclude a conflict.
    pub fn conflict(&self) -> Option<SkipReason> {
        match &self.evidence.match_check {
            MatchCheck::Conflict(conflict) => Some(SkipReason::Conflict {
                field: conflict.field.to_string(),
                extracted: conflict.extracted.clone(),
                resolved: conflict.resolved.clone(),
                similarity: conflict.similarity,
            }),
            _ => None,
        }
    }
}

/// Where the identifier a record was reached by came from.
///
/// Not a [`Tier`] variant, and no variant is added to that enum: every
/// `Tier` names a pass over a file, and borax-pdf knows nothing about
/// operators. A supplied identifier came from no pass at all, so the
/// distinction is drawn here, where the operator exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// An extraction pass over the file found it.
    Extracted(Tier),
    /// The operator typed it.
    Supplied,
    /// The file's library item supplied the whole record, so no
    /// identifier was looked for.
    Library,
}

impl Provenance {
    /// The word the `resolved` event's `tier` field carries for it: the
    /// pass's own name, `supplied`, or `library`.
    ///
    /// A bare `supplied` rather than "supplied by hand": the other
    /// values in that field name where the identifier was read, and
    /// this one names that it was not read.
    pub fn as_str(self) -> &'static str {
        match self {
            Provenance::Extracted(tier) => tier.as_str(),
            Provenance::Supplied => "supplied",
            Provenance::Library => "library",
        }
    }
}

/// What a run decided about one file.
///
// One of these is produced per file and moved once, so the bytes
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

/// The content-index pass: the record the index holds for a file, and
/// what reading the index came to.
///
/// `hash` is the file's content hash, or the message of the error that
/// kept it from being computed. In order:
///
/// - a file with no hash gives no record and [`IndexRead::Unavailable`]
///   with that message, whatever [`ResolveConfig::cache`] says;
/// - [`ResolveConfig::cache`] `false` — the `--no-cache` bypass — gives
///   no record and [`IndexRead::Bypassed`], and the index is not read;
/// - otherwise the index is read: [`IndexRead::Hit`] with its record,
///   or [`IndexRead::Miss`].
///
/// A hit means the file need not be opened at all, since a record is
/// served for its content under any name.
pub fn index_read<C: Cache>(
    hash: Result<&ContentHash, &str>,
    index: &ContentIndex<C>,
    config: &ResolveConfig,
) -> (Option<Record>, IndexRead) {
    let hash = match hash {
        Ok(hash) => hash,
        Err(message) => {
            return (
                None,
                IndexRead::Unavailable {
                    message: message.to_string(),
                },
            );
        }
    };
    if !config.cache {
        return (None, IndexRead::Bypassed);
    }
    match index.get(hash) {
        Some(record) => (Some(record), IndexRead::Hit),
        None => (None, IndexRead::Miss),
    }
}

/// The record a content-index hit stands for, with `library` as what
/// the library said before the index was read.
///
/// Nothing was read from the file and nothing was looked up, so every
/// later step is not attempted because the index answered.
fn indexed_record(record: Record, hash: Option<ContentHash>, library: Consultation) -> FileRecord {
    let reason = Unattempted::ContentIndexHit;
    FileRecord {
        record,
        hash,
        evidence: Evidence {
            library,
            content_index: IndexEvidence {
                read: IndexRead::Hit,
                write: IndexWrite::NotAttempted(reason),
            },
            ..Evidence::not_attempted(reason)
        },
        // A record served from the index was accepted by whatever run
        // put it there, not by this one.
        overrode: None,
    }
}

/// The record the library supplies for a file it tracks: its item's,
/// with `answer` as the library's answer.
///
/// Nothing was read from the file and nothing was looked up, so every
/// later step, the content index included, is not attempted because
/// the library answered.
fn library_record(record: Record, hash: Option<ContentHash>, answer: LibraryAnswer) -> FileRecord {
    FileRecord {
        record,
        hash,
        evidence: Evidence {
            library: Consultation::Consulted(answer),
            ..Evidence::not_attempted(Unattempted::LibraryAnswered)
        },
        // A library item is past the point of a conflict check: it was
        // admitted, adopted or corrected, and nothing was overridden
        // here to reach it.
        overrode: None,
    }
}

/// What the second pass read from one file: the extractor's result and
/// the titles the file claims, each kept whatever became of the other.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRead {
    /// The identifier the tiered extractor found, or why it found none.
    pub extracted: Result<Extracted, ExtractionError>,
    /// The titles the file's own metadata claims: [`Titles::Read`]
    /// whenever the file opened, and [`Titles::Failed`] when it did not.
    pub titles: Titles,
}

/// The second pass: what the file at `path` says about itself.
///
/// The file is opened once. When it opens, its titles are read before
/// the identifier is looked for, and are kept as [`Titles::Read`]
/// whatever extraction then returns — including a page that cannot be
/// read — so a file with no identifier still has claims, and an
/// operator supplying one for it has something to compare the record
/// against. When it does not open, `extracted` is the open error and
/// `titles` is [`Titles::Failed`] with that error's message.
///
/// See [`titles_of`] for the titles on their own.
pub fn from_file(path: &Path, documents: &dyn Documents, config: &ExtractionConfig) -> FileRead {
    match documents.open(path) {
        Ok(pdf) => FileRead {
            titles: Titles::Read(claimed_titles(pdf.as_ref())),
            extracted: extract(pdf.as_ref(), config),
        },
        Err(error) => FileRead {
            titles: Titles::Failed {
                message: message_of(&error),
            },
            extracted: Err(error),
        },
    }
}

/// What extraction made of the file at `path`, in the reported
/// vocabulary: [`from_file`]'s result with the titles dropped, passed
/// through [`extraction_of`].
///
/// Opens `path` and no other file, runs the passes resolution runs
/// under `config`, asks no service, reads and writes no cache, and
/// writes nothing to a library. Never fails: a file that
/// cannot be opened, or yields no identifier, is a result like any
/// other.
pub fn extraction(path: &Path, documents: &dyn Documents, config: &ExtractionConfig) -> Extraction {
    extraction_of(&from_file(path, documents, config).extracted)
}

/// `result` in the reported vocabulary, one variant for each outcome
/// the extractor tells apart.
///
/// The identifier is written as the stream writes one (`doi:…`,
/// `arXiv:…`) and the pass by its own name. Each failure maps to its
/// own variant, and an unreadable file keeps the reader's message
/// unchanged.
pub fn extraction_of(result: &Result<Extracted, ExtractionError>) -> Extraction {
    match result {
        Ok(Extracted { identifier, tier }) => Extraction::Found {
            identifier: Identifier::from(identifier.clone()).to_string(),
            tier: tier.as_str().to_string(),
        },
        Err(ExtractionError::NoTextLayer) => Extraction::NoTextLayer,
        Err(ExtractionError::NoIdentifierFound) => Extraction::TextWithoutIdentifier,
        Err(ExtractionError::Encrypted) => Extraction::Encrypted,
        Err(ExtractionError::Unreadable { message }) => Extraction::Unreadable {
            message: message.clone(),
        },
    }
}

/// The third pass: the record `sources` hold for `identifier`.
///
/// [`borax_sources::dispatch::resolve`] under the name the other passes
/// go by, so a caller driving the passes one at a time reads them as
/// one sequence. The services are consulted in priority order for the
/// identifier's kind, and where the identifier came from makes no
/// difference to any of it.
pub fn from_sources(
    sources: &[&dyn Source],
    identifier: &Identifier,
) -> Result<Resolved, Unresolved> {
    resolve(sources, identifier)
}

/// The fourth pass: what the file's own `titles` say against `record`.
///
/// [`borax_sources::conflict::check_title`] over the titles read, with
/// its conclusion carried over unchanged. Titles that could not be read
/// are no titles, and so
/// [`borax_sources::conflict::Insufficient::NoTitles`]; titles that were
/// not attempted make the check not attempted for the same reason.
pub fn title_check(titles: &Titles, record: &Record) -> MatchCheck {
    if let Titles::NotAttempted(reason) = titles {
        return MatchCheck::NotAttempted(*reason);
    }

    let claimed: Vec<&str> = titles
        .claims()
        .iter()
        .map(|claim| claim.title.as_str())
        .collect();
    match check_title(&claimed, record) {
        TitleCheck::Agreed => MatchCheck::Agreed,
        TitleCheck::Conflict(conflict) => MatchCheck::Conflict(conflict),
        TitleCheck::Insufficient(insufficient) => MatchCheck::Insufficient(insufficient),
    }
}

/// Resolve one file, asking no library.
///
/// The passes, in order, each ending the run when it succeeds:
///
/// 1. **Library**: where the run has a library ([`standing`]'s
///    `library`) and it tracks the file, the file's record is its
///    item's, returned without opening the file, asking a service, or
///    reading or writing the content index. Asked whatever
///    [`ResolveConfig::cache`] says, since the library is not a cache.
///    A library that cannot answer for the file says why, and the
///    passes below run as for a file it does not track. This function
///    passes no library, so it starts at the content index.
/// 2. **Content index**: the file's hash is looked up ([`index_read`]),
///    and a hit is returned without opening the file at all. Skipped
///    entirely when [`ResolveConfig::cache`] is `false`, and treated as
///    a miss when the file cannot be hashed — a hash failure is not yet
///    a reason to give up, since opening the file reports a better one.
/// 3. **Extraction**: [`borax_pdf::tiered::extract`] over the opened
///    file.
/// 4. **Resolution**: [`borax_sources::dispatch::resolve`] over
///    `sources`, which are consulted in priority order for the
///    identifier's type.
/// 5. **Conflict check**: the file's own titles, when it has any, are
///    compared against the resolved record's ([`title_check`]). A
///    disagreement is a skip, not a result: a record for the wrong work
///    is worse than no record.
///
/// A successful resolution by the last three passes is written to the
/// content index under the file's hash, so a later run recognises the
/// file under any name. A record the conflict check refused is never
/// written, and what the index held for the hash is left as it was. A
/// write that fails is kept as evidence and changes nothing else.
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
///
/// The verdict of [`standing`], which runs the same passes and keeps
/// what they produced along the way. A caller that acts on the verdict
/// alone wants this one.
pub fn resolve_file<C: Cache>(
    path: &Path,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    config: &ResolveConfig,
) -> FileOutcome {
    standing(path, documents, sources, index, config, None, None).verdict
}

/// Everything the passes produced about one file, for a caller that
/// may put the verdict to somebody instead of acting on it.
///
/// [`resolve_file`]'s working, kept rather than dropped. A batch run
/// wants the verdict and nothing else; a run with an operator at it
/// needs more, each for one decision it has to make:
///
/// - the hash, because a file nobody knows the hash of cannot be
///   renamed and so is not asked about;
/// - the record the conflict check refused, because the operator may
///   accept it;
/// - the evidence, because an outage is worth trying again and a
///   confirmed absence is not
///   ([`crate::evidence::LookupEvidence::is_conclusive`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    /// What a run that asked nobody would report for the file.
    pub verdict: FileOutcome,
    /// The file's content hash, or `None` when it could not be
    /// computed.
    pub hash: Option<ContentHash>,
    /// The evidence for `verdict`, with every step that ran before it.
    /// Equal to the evidence of the record the verdict is about, where
    /// there is one: the resolved record, the refused one, or the one a
    /// work duplicate resolved to.
    pub evidence: Evidence,
    /// The record the conflict check refused, with the claims it
    /// disagrees with. `Some` exactly when `verdict` is
    /// [`SkipReason::Conflict`], since that is the one verdict holding
    /// a record a run declined to use.
    pub refused: Option<FileRecord>,
    /// The work the library already holds a file for, with the record
    /// this file resolved to. `Some` exactly when the library's own
    /// work check made the verdict a duplicate, which is the one
    /// verdict an operator may answer by filing the file anyway: doing
    /// so admits it under this record, as another artifact of that
    /// work.
    pub duplicated: Option<Duplicated>,
}

/// A work the library already holds a file for, and the record an
/// incoming second file of it resolved to.
///
/// The two travel together because the question put about one needs
/// both: the work is what the operator is deciding against, and the
/// record is what filing the file would admit it under.
#[derive(Debug, Clone, PartialEq)]
pub struct Duplicated {
    pub file: FileRecord,
    pub work: WorkDuplicate,
}

impl Standing {
    /// What the run's library said about the file, or `None` when it
    /// was not asked: no library was given, the file lies outside it,
    /// or the verdict was reached before it was asked. A skip reports
    /// it as a resolved record does.
    pub fn library(&self) -> Option<&LibraryAnswer> {
        self.evidence.library.answer()
    }

    /// The standing of `file` once the library's work check has had
    /// its say: the file's verdict, with `file`'s evidence as its own.
    fn of(path: &Path, file: FileRecord, account: Option<&Account<'_>>) -> Standing {
        let hash = file.hash.clone();
        let evidence = file.evidence.clone();
        let (verdict, duplicated) = admissible(path, file, account);
        Standing {
            verdict,
            hash,
            evidence,
            refused: None,
            duplicated,
        }
    }

    /// A skip reached with no record, carrying `evidence`.
    fn skipped(reason: SkipReason, hash: Option<ContentHash>, evidence: Evidence) -> Standing {
        Standing {
            verdict: FileOutcome::Skipped(reason),
            hash,
            evidence,
            refused: None,
            duplicated: None,
        }
    }
}

/// The skip reporting the same bytes as the file at `path` — whose
/// hash is `hash` — already held by `account`.
///
/// A file with no hash is no duplicate: there is nothing to match on.
fn content_duplicate(
    path: &Path,
    hash: Option<&ContentHash>,
    account: &Account<'_>,
) -> Option<SkipReason> {
    Some(duplicate(
        DuplicateReason::Content,
        &account.content_duplicate(path, hash?)?,
    ))
}

/// The work the library already holds a file of, for a record the
/// operator reached rather than one the run resolved on its own.
///
/// The same work check [`standing`] makes, asked where a record arrives
/// having bypassed it: supplied, retried, or accepted over a conflict.
/// `None` leaves the record to be acted on.
///
/// The caller decides what the answer means. A record the run resolved
/// on its own makes the file a work duplicate, which a batch run skips
/// and an interactive one offers to file; a record the operator reached
/// is the operator saying what the file is, so the collision is
/// reported to them rather than deciding anything.
pub fn second_copy(
    path: &Path,
    record: &Record,
    account: Option<&Account<'_>>,
) -> Option<WorkDuplicate> {
    account?.work_duplicate(path, &identifiers_of(record))
}

/// The skip reporting the file at `existing` as a duplicate of
/// `reason`'s kind.
fn duplicate(reason: DuplicateReason, existing: &Path) -> SkipReason {
    SkipReason::Duplicate {
        reason,
        existing_path: existing.to_path_buf(),
    }
}

/// Resolve one file, keeping the working [`resolve_file`] discards.
///
/// The passes in the order [`resolve_file`] documents, with the
/// library's two duplicate checks around them where the run keeps an
/// account — before anything else is asked, on the file's content, and
/// once a record is in hand, on the record's identifiers. `None` is a
/// run that admits nothing anywhere, which runs neither check rather
/// than running both against an empty store.
///
/// `library` is the run's library as read, asked about the file after
/// the content check and before the content index. A record it answers
/// with goes through the work check as any other does. `None` asks no
/// library, and so does a file outside the one given; the evidence
/// says which.
///
/// The file is hashed once, whether or not the index is consulted and
/// whether or not a duplicate check or the library wants it: the hash
/// identifies the file for every later decision about it.
///
/// Every verdict carries the evidence of the steps that ran before it,
/// and says of each step that did not run why it did not
/// ([`Standing::evidence`]).
pub fn standing<C: Cache>(
    path: &Path,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    config: &ResolveConfig,
    account: Option<&Account<'_>>,
    library: Option<&Stores>,
) -> Standing {
    let hashed = documents.hash(path);
    let hash = hashed.as_ref().ok().cloned();
    if let Some(duplicate) =
        account.and_then(|account| content_duplicate(path, hash.as_ref(), account))
    {
        return Standing::skipped(
            duplicate,
            hash,
            Evidence::not_attempted(Unattempted::ContentDuplicate),
        );
    }

    let consulted = match library {
        None => Err(Unattempted::NoLibrary),
        Some(stores) => stores
            .consult(path, hash.as_ref())
            .ok_or(Unattempted::OutsideLibrary),
    };
    match consulted {
        Ok(Consulted {
            answer,
            item: Some(item),
        }) => Standing::of(path, library_record(item.record, hash, answer), account),
        Ok(Consulted { answer, item: None }) => beyond_library(
            path,
            hashed.as_ref().map_err(message_of),
            documents,
            sources,
            index,
            config,
            account,
            Consultation::Consulted(answer),
        ),
        Err(reason) => beyond_library(
            path,
            hashed.as_ref().map_err(message_of),
            documents,
            sources,
            index,
            config,
            account,
            Consultation::NotConsulted(reason),
        ),
    }
}

/// The passes after the library, for a file whose hash is `hashed` —
/// or the message of the error that kept it from being computed — and
/// which the library did not answer for: `library` is what it said
/// instead, or why it was not asked, carried onto the evidence of
/// every verdict reached here.
#[allow(clippy::too_many_arguments)]
fn beyond_library<C: Cache>(
    path: &Path,
    hashed: Result<&ContentHash, String>,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    config: &ResolveConfig,
    account: Option<&Account<'_>>,
    library: Consultation,
) -> Standing {
    let hash = hashed.as_ref().ok().map(|hash| (*hash).clone());
    let (indexed, read) = index_read(
        hashed.as_ref().map(|hash| *hash).map_err(String::as_str),
        index,
        config,
    );
    if let Some(record) = indexed {
        return Standing::of(path, indexed_record(record, hash, library), account);
    }

    let FileRead { extracted, titles } = from_file(path, documents, &config.extraction);
    let extraction = ExtractionEvidence {
        result: ExtractionStep::Ran(extraction_of(&extracted)),
        titles,
    };
    let Extracted { identifier, tier } = match extracted {
        Ok(extracted) => extracted,
        Err(error) => {
            let reason = Unattempted::ExtractionFailed;
            let evidence = Evidence {
                library,
                content_index: IndexEvidence {
                    read,
                    write: IndexWrite::NotAttempted(reason),
                },
                extraction,
                lookup: LookupEvidence::NotAttempted(reason),
                match_check: MatchCheck::NotAttempted(reason),
            };
            return Standing::skipped(skipped_for(&error), hash, evidence);
        }
    };
    let looked_up = Identifier::from(identifier);
    let origin = Origin::Extracted(tier);

    let resolved = match from_sources(sources, &looked_up) {
        Ok(resolved) => resolved,
        Err(unresolved) => {
            let reason = Unattempted::NoRecord;
            let evidence = Evidence {
                library,
                content_index: IndexEvidence {
                    read,
                    write: IndexWrite::NotAttempted(reason),
                },
                extraction,
                lookup: unheld_lookup(&looked_up, origin, &unresolved),
                match_check: MatchCheck::NotAttempted(reason),
            };
            let reason = unresolvable(&unresolved, &looked_up, tier);
            return Standing::skipped(reason, hash, evidence);
        }
    };

    let match_check = title_check(&extraction.titles, &resolved.record);
    let refused = matches!(match_check, MatchCheck::Conflict(_));
    // A refused record is never written under the file's hash: a later
    // run checks the file again rather than being answered for it.
    let write = match (refused, hash.as_ref()) {
        (true, _) => IndexWrite::NotAttempted(Unattempted::Refused),
        (false, Some(hash)) => IndexWrite::Attempted(index.put(hash, &resolved.record)),
        (false, None) => IndexWrite::NotAttempted(Unattempted::Unhashable),
    };
    let file = FileRecord {
        hash: hash.clone(),
        evidence: Evidence {
            library,
            content_index: IndexEvidence { read, write },
            extraction,
            lookup: found_lookup(looked_up, origin, &resolved),
            match_check,
        },
        record: resolved.record,
        // Nothing has been overridden: either the conflict check has
        // passed, or this record is refused below and whoever accepts
        // it records what they accepted it over.
        overrode: None,
    };

    if let Some(conflict) = file.conflict() {
        let evidence = file.evidence.clone();
        return Standing {
            refused: Some(file),
            ..Standing::skipped(conflict, hash, evidence)
        };
    }

    Standing::of(path, file, account)
}

/// `file` as the verdict it is, unless the library already holds
/// another file for the same work.
///
/// The later of the two duplicate checks: it needs a record, so it is
/// answerable only once one is in hand, which is the earliest a second
/// PDF of one paper can be recognised at all.
///
/// The work comes back beside the verdict as well as in it, since the
/// file is one an operator may yet file as another artifact of that
/// work and the question about it is put from both.
fn admissible(
    path: &Path,
    file: FileRecord,
    account: Option<&Account<'_>>,
) -> (FileOutcome, Option<Duplicated>) {
    match account.and_then(|account| account.work_duplicate(path, &identifiers_of(&file.record))) {
        Some(work) => (
            FileOutcome::Skipped(duplicate(DuplicateReason::Work, &work.existing)),
            Some(Duplicated { file, work }),
        ),
        None => (FileOutcome::Resolved(file), None),
    }
}

/// What a file says about itself, read from the file rather than from
/// a record.
///
/// Available without resolving anything, which is what a caller needs
/// when it is about to compare a record it was handed against the file
/// it is for: a file the content index answered for was never opened.
///
/// [`Titles::Read`] with every title the file claims, possibly none,
/// when it opens; [`Titles::Failed`] with the open error's message when
/// it does not. Never [`Titles::NotAttempted`].
pub fn titles_of(path: &Path, documents: &dyn Documents) -> Titles {
    match documents.open(path) {
        Ok(pdf) => Titles::Read(claimed_titles(pdf.as_ref())),
        Err(error) => Titles::Failed {
            message: message_of(&error),
        },
    }
}

/// Resolve `identifier` for the file at `path`, as a run resolves one
/// it found itself, on top of the file's existing evidence `prior`.
///
/// The same services in the same order, and the same title check
/// against the file's own titles, read now — but the check is reported
/// ([`FileRecord::conflict`]) rather than enforced: an identifier a
/// person supplied is a stronger statement than the heuristic that
/// would refuse it, and what the caller does about a disagreement is
/// the caller's to decide.
///
/// `origin` is where `identifier` came from: [`Origin::Operator`] for
/// one the operator supplied, and the file's own extraction pass for a
/// lookup of its own identifier asked again.
///
/// The record's evidence is `prior` with four sections replaced: the
/// lookup, with every attempt; the titles, read now; the title check
/// over them; and the content-index write, which waits for acceptance
/// ([`Unattempted::AwaitingAcceptance`]). What the library said, what
/// the index read came to and what extraction found stay as `prior`
/// has them.
///
/// Nothing is written to the content index here. A record reached this
/// way is a candidate until somebody accepts it, and [`remember`] is
/// what keeps one that was accepted.
///
/// Fails with what every service answered when none holds the
/// identifier; [`unheld_evidence`] gives that lookup its evidence.
pub fn resolve_supplied(
    path: &Path,
    identifier: &Identifier,
    origin: Origin,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    prior: &Evidence,
) -> Result<FileRecord, Unresolved> {
    let resolved = from_sources(sources, identifier)?;
    // Read now rather than carried in: a file the content index
    // answered for was never opened, and one no identifier was found in
    // was never asked about its titles, so the comparison has nothing
    // to work from until it is wanted.
    let titles = titles_of(path, documents);
    let match_check = title_check(&titles, &resolved.record);

    Ok(FileRecord {
        // Taken here because a rename needs it: the planner recognises
        // an already-named file by content, and a record the operator
        // then accepts is remembered under it.
        hash: documents.hash(path).ok(),
        evidence: Evidence {
            library: prior.library.clone(),
            content_index: IndexEvidence {
                read: prior.content_index.read.clone(),
                write: IndexWrite::NotAttempted(Unattempted::AwaitingAcceptance),
            },
            extraction: ExtractionEvidence {
                result: prior.extraction.result.clone(),
                titles,
            },
            lookup: found_lookup(identifier.clone(), origin, &resolved),
            match_check,
        },
        record: resolved.record,
        // Nothing has been overridden yet. Accepting this record over a
        // conflict is the caller's decision, and [`accept`] records it.
        overrode: None,
    })
}

/// The evidence of a lookup of `identifier`, which came from `origin`,
/// that no service answered: `prior` with the lookup set to
/// `unresolved`'s attempts, in order, and the title check and the
/// content-index write not attempted because there is no record
/// ([`Unattempted::NoRecord`]). Every other section is `prior`'s.
pub fn unheld_evidence(
    prior: &Evidence,
    identifier: &Identifier,
    origin: Origin,
    unresolved: &Unresolved,
) -> Evidence {
    let reason = Unattempted::NoRecord;
    Evidence {
        library: prior.library.clone(),
        content_index: IndexEvidence {
            read: prior.content_index.read.clone(),
            write: IndexWrite::NotAttempted(reason),
        },
        extraction: prior.extraction.clone(),
        lookup: unheld_lookup(identifier, origin, unresolved),
        match_check: MatchCheck::NotAttempted(reason),
    }
}

/// `file` as an operator's accepting it makes it.
///
/// `overrode` is the conflict the title check concluded, in the
/// vocabulary the skip would have used, or `None` when it concluded
/// none. A content-index write held back because the record was
/// refused ([`Unattempted::Refused`]) now waits for the move instead
/// ([`Unattempted::AwaitingAcceptance`]). Nothing else changes: the
/// record, its hash, and the title check's conclusion stay as they
/// were.
pub fn accept(file: FileRecord) -> FileRecord {
    let overrode = match &file.evidence.match_check {
        MatchCheck::Conflict(conflict) => Some(Overridden {
            field: conflict.field.to_string(),
            extracted: conflict.extracted.clone(),
            resolved: conflict.resolved.clone(),
            similarity: conflict.similarity,
        }),
        _ => None,
    };
    let mut accepted = FileRecord { overrode, ..file };
    let write = &mut accepted.evidence.content_index.write;
    if *write == IndexWrite::NotAttempted(Unattempted::Refused) {
        *write = IndexWrite::NotAttempted(Unattempted::AwaitingAcceptance);
    }
    accepted
}

/// Keep `record` as what the file at `hash` is, so that no later run
/// asks about it again, and say what became of the write.
///
/// [`IndexWrite::Attempted`] with the store's result, or
/// [`IndexWrite::NotAttempted`] with [`Unattempted::Unhashable`] when
/// the file has no hash to be remembered under.
///
/// Best-effort, as every write to the response cache is: an entry that
/// cannot be written leaves the file to be asked about next time,
/// which is the safe way to lose an answer, and a failed write is
/// never a failure of the rename it followed.
pub fn remember<C: Cache>(
    index: &ContentIndex<C>,
    hash: Option<&ContentHash>,
    record: &Record,
) -> IndexWrite {
    match hash {
        Some(hash) => IndexWrite::Attempted(index.put(hash, record)),
        None => IndexWrite::NotAttempted(Unattempted::Unhashable),
    }
}

/// The lookup of `identifier`, which came from `origin`, that reached
/// `resolved`: every service that failed first, then the one that
/// answered.
fn found_lookup(identifier: Identifier, origin: Origin, resolved: &Resolved) -> LookupEvidence {
    let failed = resolved
        .failures
        .iter()
        .map(|(service, error)| ServiceAttempt {
            service: *service,
            outcome: Err(error.clone()),
        });
    let found = ServiceAttempt {
        service: resolved.source,
        outcome: Ok(resolved.retrieval.clone()),
    };
    LookupEvidence::Attempted {
        identifier,
        origin,
        attempts: failed.chain([found]).collect(),
    }
}

/// The lookup of `identifier`, which came from `origin`, that no
/// service answered: `unresolved`'s attempts, in order.
fn unheld_lookup(
    identifier: &Identifier,
    origin: Origin,
    unresolved: &Unresolved,
) -> LookupEvidence {
    LookupEvidence::Attempted {
        identifier: identifier.clone(),
        origin,
        attempts: unresolved
            .attempts
            .iter()
            .map(|(service, error)| ServiceAttempt {
                service: *service,
                outcome: Err(error.clone()),
            })
            .collect(),
    }
}

/// The text a reader is given for `error`: an unreadable file's own
/// message, and the error's description otherwise.
fn message_of(error: &ExtractionError) -> String {
    match error {
        ExtractionError::Unreadable { message } => message.clone(),
        other => other.to_string(),
    }
}

/// Every identifier `record` carries, in the order DOI, arXiv, PMID,
/// ISBN — the order [`crate::library::admit`] searches the item store
/// in, so an incoming file's identifiers are checked in the order an
/// admitted one's are looked up.
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

/// Every title an open document claims for itself, in the order they
/// are read.
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
fn claimed_titles(pdf: &dyn PdfSource) -> Vec<Claim> {
    [
        (ClaimOrigin::Xmp, pdf.xmp().and_then(xmp_title)),
        (ClaimOrigin::Info, pdf.info_metadata().title.clone()),
    ]
    .into_iter()
    .filter_map(|(from, title)| {
        Some(Claim {
            from,
            title: title?,
        })
    })
    .collect()
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
pub fn unresolvable(unresolved: &Unresolved, found: &Identifier, tier: Tier) -> SkipReason {
    SkipReason::Unresolvable {
        found: found.to_string(),
        tier: Some(tier.as_str().to_string()),
        attempts: attempts_of(unresolved),
    }
}

/// What each service answered, as a reader of the stream is told it:
/// one entry per source, in the order they were asked.
///
/// Separate from [`unresolvable`] because a caller that is showing a
/// failed lookup rather than reporting it has no skip to build — an
/// identifier the operator supplied and nobody held is a candidate's
/// outcome and not the file's verdict.
pub fn attempts_of(unresolved: &Unresolved) -> Vec<Attempt> {
    unresolved
        .attempts
        .iter()
        .map(|(source, error)| Attempt {
            source: source.to_string(),
            error: error.to_string(),
        })
        .collect()
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
/// A record whose provenance names no such source reports where it was
/// kept instead, since nothing else is known about where it came from:
/// `library` for a record the library supplied, and `cache` for one the
/// content index did.
fn sources_of(file: &FileRecord) -> String {
    if let Some(source) = file.source() {
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

    match (named.is_empty(), file.tier()) {
        (false, _) => named.join(", "),
        (true, Some(Provenance::Library)) => "library".to_string(),
        (true, _) => "cache".to_string(),
    }
}

/// The event that reports `outcome` for `path`.
///
/// A resolved file whose source is unknown — the content index
/// answered — reports its source as `cache`. A skip carries no library
/// answer, having no record to carry one; [`verdict_event`] is the one
/// that reports a skip's.
pub fn event_for(path: &Path, outcome: &FileOutcome) -> Event {
    match outcome {
        FileOutcome::Resolved(file) => resolved_event(path, file),
        FileOutcome::Skipped(reason) => Event::Skipped {
            path: path.to_path_buf(),
            reason: reason.clone(),
            library: None,
        },
    }
}

/// The event reporting `standing`'s verdict for `path`, carrying its
/// library answer on a `skipped` event as on a `resolved` one.
pub fn verdict_event(path: &Path, standing: &Standing) -> Event {
    match &standing.verdict {
        FileOutcome::Resolved(file) => resolved_event(path, file),
        FileOutcome::Skipped(reason) => Event::Skipped {
            path: path.to_path_buf(),
            reason: reason.clone(),
            library: standing.library().cloned(),
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
            .found()
            .map_or_else(|| identifier_of(&file.record), Identifier::to_string),
        claims: file.claims().to_vec(),
        tier: file.tier().map(|whence| whence.as_str().to_string()),
        overrode: file.overrode.clone(),
        cached: file.cached(),
        library: file.library().cloned(),
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
/// `library` is the run's library, asked about every file as
/// [`standing`] asks it and shared by every worker: nothing is learned
/// into it, so no file's answer depends on another's.
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
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    library: Option<&Stores>,
    config: &(dyn Fn(&Path) -> ResolveConfig + Sync),
    concurrency: usize,
) -> Run {
    let mut events = map_bounded(paths.to_vec(), concurrency, |path| {
        let standing = standing(
            &path,
            documents,
            sources,
            index,
            &config(&path),
            None,
            library,
        );
        verdict_event(&path, &standing)
    });

    let mut counts = Counts::default();
    for event in &events {
        counts.observe(event);
    }

    events.push(Event::RunFinished { counts });
    Run { events, counts }
}

/// [`Documents`] backed by the real filesystem and the pure-Rust PDF
/// engine.
#[derive(Debug, Clone, Copy)]
pub struct RealDocuments;

impl Documents for RealDocuments {
    /// The file's content hash, read in chunks so a large PDF is never
    /// held in memory whole.
    ///
    /// A file that cannot be read reports
    /// [`ExtractionError::Unreadable`] carrying the I/O error, which
    /// [`resolve_file`] records as the content index being unavailable
    /// and goes on to open the file, rather than taking it as a verdict.
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        hash_file(path).map_err(|error| ExtractionError::Unreadable {
            message: error.to_string(),
        })
    }

    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        Ok(Box::new(PurePdf::open(path)?))
    }
}
