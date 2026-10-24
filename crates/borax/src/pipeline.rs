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
use borax_core::library::{DuplicateReason, EvidenceOrigin, IdentificationEvidence, Item, ItemId};
use borax_core::record::Record;
use borax_pdf::pure::PurePdf;
use borax_pdf::scan::xmp_title;
use borax_pdf::source::{ExtractionError, PdfSource};
use borax_pdf::tiered::{Extracted, ExtractionConfig, extract};
use borax_sources::conflict::{Conflict, TitleCheck, check_title};
use borax_sources::dispatch::{Resolved, Unresolved, resolve};
use borax_sources::pace::map_bounded;
use borax_sources::source::Source;
use borax_sources::store::{ContentIndex, IndexStore, hash_file};

use crate::event::{
    Acceptance, Claim, ClaimOrigin, Counts, Event, Extraction, LibraryAnswer, ServiceAnswer,
    SkipReason,
};
use crate::evidence::{
    Consultation, Evidence, ExtractionEvidence, ExtractionStep, IdentifierInput, IndexEvidence,
    IndexRead, IndexWrite, LookupEvidence, LookupRound, MatchCheck, Origin, ServiceAttempt, Titles,
    Unattempted,
};
use crate::library::{Account, Consulted, Stores, WorkDuplicate};
use crate::references::{Libraries, ReferenceRead};
use crate::refresh::{Decision, Refresh, Whose, refresh, whose};

/// The documents a run works on, as something that can be read.
///
/// Not the library a run writes to: this is the seam file bytes are
/// read through, and [`crate::library`] is the tree they sit in.
///
/// The one seam to the filesystem. A run hashes a file before it opens
/// it, because a hash its library tracks, or one the content index
/// answers for, makes opening it unnecessary.
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
    /// file, step by step.
    pub evidence: Evidence,
    /// Whether an operator accepted `record` over the conflict its
    /// title check concluded, which `evidence.match_check` still holds.
    ///
    /// Only an interactive run can set it ([`accept`]): the batch path
    /// skips every conflict it finds, so a record that reaches a caller
    /// from there has nothing to have overridden.
    pub overridden: bool,
    /// Whether an operator accepted `record`. Set by [`accept`] alone,
    /// so it is `false` on every record the batch path reaches; only a
    /// record an operator supplied reads it.
    pub accepted: bool,
    /// The fresh lookup of the library item the file's own resolution
    /// reached, and what became of its candidate: `Some` exactly in a
    /// run given `--refresh` ([`ResolveConfig::refresh`]) for a file
    /// that resolved from an item, the run's library's or another's
    /// through a reference. `None` for every other verdict, a file
    /// answered by the content index's evidence or by extraction
    /// included, since its own lookup is the fresh one.
    pub refresh: Option<Refresh>,
}

impl FileRecord {
    /// What the run's library said about the file, or `None` when it
    /// was not asked.
    ///
    /// [`LibraryAnswer::Tracked`] with a lookup not attempted because
    /// the library answered is a record the library supplied; with a
    /// lookup, the library answered and the record was reached some
    /// other way, as when an operator re-identified the file. A
    /// problem answer is a record reached by the passes the library
    /// could not spare.
    pub fn library(&self) -> Option<&LibraryAnswer> {
        self.evidence.library.answer()
    }

    /// The conflict the title check concluded between the record and
    /// the file's own titles, or `None` when it concluded none. Still
    /// reported for a record an operator accepted over it.
    pub fn conflict(&self) -> Option<&Conflict> {
        match &self.evidence.match_check {
            MatchCheck::Conflict(conflict) => Some(conflict),
            _ => None,
        }
    }

    /// How the record was used, or is on offer to be:
    ///
    /// - [`Acceptance::Overridden`] when [`FileRecord::overridden`] is
    ///   set, whatever the record's origin;
    /// - for a record whose lookup an operator supplied,
    ///   [`Acceptance::Accepted`] once [`FileRecord::accepted`] is set
    ///   and [`Acceptance::Pending`] until then;
    /// - [`Acceptance::Automatic`] for any other record: the file's own
    ///   lookup, retried or not, a lookup of the content index's
    ///   identifier, or none.
    pub fn acceptance(&self) -> Acceptance {
        let supplied = matches!(
            self.evidence.lookup,
            LookupEvidence::Attempted {
                origin: Origin::Operator,
                ..
            }
        );
        match (self.overridden, supplied, self.accepted) {
            (true, _, _) => Acceptance::Overridden,
            (false, true, true) => Acceptance::Accepted,
            (false, true, false) => Acceptance::Pending,
            (false, false, _) => Acceptance::Automatic,
        }
    }

    /// The evidence the record identifies the file's bytes by, or
    /// `None` when no lookup reached it.
    ///
    /// The identifier is the lookup's, and the origin is, the first
    /// that holds:
    ///
    /// - `overridden` when [`FileRecord::overridden`] is set;
    /// - `supplied` for a lookup an operator supplied;
    /// - `extracted` for the file's own lookup, retried or not;
    /// - the answering fact's own origin for a lookup of the content
    ///   index's identifier.
    ///
    /// A record the library supplied, or a reference to another library
    /// answered with, was reached by no lookup, and has none.
    pub fn identification(&self) -> Option<IdentificationEvidence> {
        let LookupEvidence::Attempted {
            identifier, origin, ..
        } = &self.evidence.lookup
        else {
            return None;
        };
        let origin = match (self.overridden, origin) {
            (true, _) => EvidenceOrigin::Overridden,
            (false, Origin::Operator) => EvidenceOrigin::Supplied,
            (false, Origin::Extracted(_)) => EvidenceOrigin::Extracted,
            (false, Origin::Indexed(origin)) => *origin,
        };
        Some(IdentificationEvidence {
            identifier: identifier.clone(),
            origin,
        })
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
    /// `--no-cache` bypass: every untracked file is extracted and every
    /// identifier it yields asked about again. It does not bypass the
    /// library, so a file an item answers for still asks nothing unless
    /// [`ResolveConfig::refresh`] is set.
    pub cache: bool,
    /// Whether the run was given `--refresh`: a file a library item
    /// answers for still resolves to the item's record, and the item's
    /// preferred identifier is looked up again for a fresh record to
    /// compare it with. That every lookup asks the service is the run's
    /// sources' doing, not this setting's.
    pub refresh: bool,
}

/// The content-index pass: what reading the index for a file came to.
///
/// `hash` is the file's content hash, or the message of the error that
/// kept it from being computed. In order:
///
/// - a file with no hash gives [`IndexRead::Unavailable`] with that
///   message, whatever [`ResolveConfig::cache`] says;
/// - [`ResolveConfig::cache`] `false` — the `--no-cache` bypass — gives
///   [`IndexRead::Bypassed`], and the index is not read;
/// - otherwise the evidence of the hash's entry is read:
///   [`IndexRead::Hit`] with the evidence it answers with
///   ([`borax_core::index::IndexEntry::answer`]), or [`IndexRead::Miss`]
///   when it answers with none.
///
/// The entry's references are not read here: [`standing`] reads them
/// first, and asks this only when none answered.
///
/// A hit means the file need not be opened at all: the evidence's
/// identifier is looked up instead of one extracted from the file.
pub fn index_read<S: IndexStore>(
    hash: Result<&ContentHash, &str>,
    index: &ContentIndex<S>,
    config: &ResolveConfig,
) -> IndexRead {
    let hash = match hash {
        Ok(hash) => hash,
        Err(message) => {
            return IndexRead::Unavailable {
                message: message.to_string(),
            };
        }
    };
    if !config.cache {
        return IndexRead::Bypassed;
    }
    match index.entry(hash).answer() {
        Some(evidence) => IndexRead::Hit(evidence),
        None => IndexRead::Miss,
    }
}

/// The standing of a file the content index answered for with
/// `evidence`, with `library` as what the library said before the index
/// was read and `references` as the entry's references, none of which
/// answered: `evidence`'s identifier looked up through `sources`, with
/// origin [`Origin::Indexed`].
///
/// Nothing is read from the file and no title check is made, whatever
/// the evidence's origin, and nothing is written to the index: each is
/// not attempted because the index answered. A record found goes
/// through the library's work check as any other does. A lookup no
/// service answered is [`SkipReason::Unresolvable`] with its attempts,
/// and the file is not extracted instead.
#[allow(clippy::too_many_arguments)]
fn indexed_standing(
    path: &Path,
    evidence: IdentificationEvidence,
    references: Vec<ReferenceRead>,
    hash: Option<ContentHash>,
    sources: &[&dyn Source],
    account: Option<&Account<'_>>,
    library: Consultation,
) -> Standing {
    let reason = Unattempted::ContentIndexHit;
    let origin = Origin::Indexed(evidence.origin);
    let identifier = evidence.identifier.clone();
    let looked_up = from_sources(sources, &identifier);
    let base = Evidence {
        library,
        content_index: IndexEvidence {
            read: IndexRead::Hit(evidence),
            references,
            write: IndexWrite::NotAttempted(reason),
        },
        identifier_input: IdentifierInput::NotAttempted(Unattempted::NotAsked),
        ..Evidence::not_attempted(reason)
    };
    match looked_up {
        Ok(resolved) => {
            let file = FileRecord {
                hash,
                evidence: Evidence {
                    lookup: found_lookup(identifier, origin, &resolved, Vec::new()),
                    ..base
                },
                record: resolved.record,
                // The decision the evidence records was made by the run
                // that wrote it, not by this one.
                overridden: false,
                accepted: false,
                refresh: None,
            };
            Standing::of(path, file, account)
        }
        Err(unresolved) => Standing::skipped(
            SkipReason::Unresolvable,
            hash,
            Evidence {
                lookup: unheld_lookup(&identifier, origin, &unresolved, Vec::new()),
                ..base
            },
        ),
    }
}

/// The standing of a file a reference to a library answered for:
/// `item`'s record, with `library` as what the run's library said and
/// `references` as the entry's references, the answering one among
/// them.
///
/// Nothing was read from the file, looked up, checked or written: each
/// of those steps is not attempted because the content index answered,
/// and no operator was asked. The record goes through the library's
/// work check as any other does. It is no lookup's, so it carries no
/// identification evidence ([`FileRecord::identification`]).
fn referenced_standing(
    path: &Path,
    item: Item,
    references: Vec<ReferenceRead>,
    hash: Option<ContentHash>,
    account: Option<&Account<'_>>,
    library: Consultation,
) -> Standing {
    let reason = Unattempted::ContentIndexHit;
    let file = FileRecord {
        record: item.record,
        hash,
        evidence: Evidence {
            library,
            content_index: IndexEvidence {
                read: IndexRead::Referenced,
                references,
                write: IndexWrite::NotAttempted(reason),
            },
            identifier_input: IdentifierInput::NotAttempted(Unattempted::NotAsked),
            ..Evidence::not_attempted(reason)
        },
        // The item is the referenced library's decision, made there.
        overridden: false,
        accepted: false,
        refresh: None,
    };
    Standing::of(path, file, account)
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
            identifier_input: IdentifierInput::NotAttempted(Unattempted::NotAsked),
            ..Evidence::not_attempted(Unattempted::LibraryAnswered)
        },
        // A library item is past the point of a conflict check: it was
        // admitted, adopted or corrected, and nothing was overridden
        // here to reach it.
        overridden: false,
        accepted: false,
        refresh: None,
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
/// 2. **Content index**: the file's hash is looked up. First its
///    references to libraries are reached and settled
///    ([`crate::references::read`]): where one answers, the file's
///    record is its item's, returned without opening the file, asking
///    a service, or writing the content index. Otherwise the evidence
///    the entry answers with is read ([`index_read`]): on a hit, its
///    identifier is looked up over `sources` without opening the file
///    or checking its titles, and what that lookup reaches is the
///    result: the record, or [`SkipReason::Unresolvable`]. Every
///    reference read is reported, whatever answered. Skipped entirely
///    when [`ResolveConfig::cache`] is `false`, and treated as a miss
///    when the file cannot be hashed — a hash failure is not yet a
///    reason to give up, since opening the file reports a better one.
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
/// A successful resolution by the last three passes writes the
/// identifier it looked up to the content index, as the extracted
/// evidence for the file's hash, so a later run recognises the file
/// under any name. The operator's evidence for the hash is never
/// touched. A record the conflict check refused writes nothing, and
/// what the index held for the hash is left as it was. A write that
/// fails is kept as evidence and changes nothing else. The write
/// happens even when [`ResolveConfig::cache`] is `false`: the bypass
/// forces a fresh extraction, and the point of forcing one is usually
/// that the stored identifier was wrong, so the fresh one replaces it.
///
/// Each extraction failure is a skip of its own kind:
/// [`ExtractionError::NoTextLayer`] is [`SkipReason::NoTextLayer`],
/// [`ExtractionError::NoIdentifierFound`] is
/// [`SkipReason::TextWithoutIdentifier`], [`ExtractionError::Encrypted`]
/// is [`SkipReason::Encrypted`], and [`ExtractionError::Unreadable`] is
/// [`SkipReason::Unreadable`] carrying the reader's message. A lookup
/// no service answered is [`SkipReason::Unresolvable`], and a record the
/// title check refused is [`SkipReason::Conflict`].
///
/// Never panics and never propagates an error: every failure is a
/// [`FileOutcome::Skipped`] carrying the reason, because one unreadable
/// file must not end a batch.
///
/// Under [`ResolveConfig::refresh`], a file that resolves from an item,
/// by the first pass or by a reference in the second, still resolves to
/// the item's record, and the item's preferred identifier is then
/// looked up over `sources` for a fresh record to compare it with
/// ([`FileRecord::refresh`]). That lookup is the only service a tracked
/// file asks, and it opens nothing, checks no title, and reads and
/// writes no content index. A verdict that is not resolved, a work
/// duplicate included, gets no refresh and asks nothing.
///
/// The verdict of [`standing`], which runs the same passes and keeps
/// what they produced along the way. A caller that acts on the verdict
/// alone wants this one.
pub fn resolve_file<S: IndexStore>(
    path: &Path,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<S>,
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

    /// This standing with its verdict's record, the record of the item
    /// `item`, refreshed over `sources` ([`refresh`]) where `config`
    /// asks for a refresh and the verdict is resolved.
    ///
    /// The candidate is offered to nobody here: its decision is not
    /// attempted, as [`Unattempted::NotAsked`] for the run's own item
    /// and [`Unattempted::AnotherLibrary`] for another library's. Any
    /// other verdict, a work duplicate included, is left as it is and
    /// asks no service.
    fn refreshed(
        mut self,
        config: &ResolveConfig,
        item: ItemId,
        whose: Whose,
        sources: &[&dyn Source],
    ) -> Standing {
        if let (true, FileOutcome::Resolved(file)) = (config.refresh, &mut self.verdict) {
            file.refresh = Some(Refresh {
                item,
                whose,
                fresh: refresh(&file.record, sources),
                decision: Decision::NotAttempted(match whose {
                    Whose::Own => Unattempted::NotAsked,
                    Whose::Referenced => Unattempted::AnotherLibrary,
                }),
            });
        }
        self
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
///
/// `hash` is the file's content hash, so the item its own record
/// answers with for those bytes is passed over as its link is.
pub fn second_copy(
    path: &Path,
    hash: Option<&ContentHash>,
    record: &Record,
    account: Option<&Account<'_>>,
) -> Option<WorkDuplicate> {
    account?.work_duplicate(path, hash, &identifiers_of(record))
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
///
/// The libraries the content index's references name are read afresh,
/// for this file alone; [`standing_in`] shares what was read across
/// files.
pub fn standing<S: IndexStore>(
    path: &Path,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<S>,
    config: &ResolveConfig,
    account: Option<&Account<'_>>,
    library: Option<&Stores>,
) -> Standing {
    standing_in(
        path,
        documents,
        sources,
        index,
        config,
        account,
        library,
        &Libraries::new(),
    )
}

/// [`standing`], reaching the libraries the content index's references
/// name through `libraries`, so that each library root is read at most
/// once across every file a run resolves through it: what a root holds
/// is what the first read of it found.
///
/// The refresh [`ResolveConfig::refresh`] asks for is attached here:
/// to a verdict the run's library answered, as the run's own item's,
/// and to one a reference answered, as whichever library's [`whose`]
/// says. Its candidate is offered to nobody.
#[allow(clippy::too_many_arguments)]
pub fn standing_in<S: IndexStore>(
    path: &Path,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<S>,
    config: &ResolveConfig,
    account: Option<&Account<'_>>,
    library: Option<&Stores>,
    libraries: &Libraries,
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
        }) => Standing::of(path, library_record(item.record, hash, answer), account).refreshed(
            config,
            item.id,
            Whose::Own,
            sources,
        ),
        Ok(Consulted { answer, item: None }) => beyond_library(
            path,
            hashed.as_ref().map_err(message_of),
            documents,
            sources,
            index,
            config,
            account,
            (Consultation::Consulted(answer), library),
            libraries,
        ),
        Err(reason) => beyond_library(
            path,
            hashed.as_ref().map_err(message_of),
            documents,
            sources,
            index,
            config,
            account,
            (Consultation::NotConsulted(reason), library),
            libraries,
        ),
    }
}

/// The passes after the library, for a file whose hash is `hashed` —
/// or the message of the error that kept it from being computed — and
/// which the library did not answer for. `library` is what it said
/// instead, or why it was not asked, carried onto the evidence of
/// every verdict reached here, beside the run's library as read, which
/// decides whose item a reference answers with ([`whose`]).
///
/// The entry's references are read first, through `libraries`, for a
/// hashed file with the cache on; a reference that answers ends the
/// passes, and its item is refreshed under [`ResolveConfig::refresh`]
/// as a library's is. The references read are carried onto every
/// verdict's evidence.
#[allow(clippy::too_many_arguments)]
fn beyond_library<S: IndexStore>(
    path: &Path,
    hashed: Result<&ContentHash, String>,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<S>,
    config: &ResolveConfig,
    account: Option<&Account<'_>>,
    (library, stores): (Consultation, Option<&Stores>),
    libraries: &Libraries,
) -> Standing {
    let hash = hashed.as_ref().ok().map(|hash| (*hash).clone());
    let (answer, references) = match (&hashed, config.cache) {
        (Ok(hash), true) => crate::references::read(index, hash, libraries),
        _ => (None, Vec::new()),
    };
    if let Some((reference, item)) = answer {
        let id = item.id.clone();
        return referenced_standing(path, item, references, hash, account, library).refreshed(
            config,
            id,
            whose(&reference, stores),
            sources,
        );
    }
    let read = index_read(
        hashed.as_ref().map(|hash| *hash).map_err(String::as_str),
        index,
        config,
    );
    if let IndexRead::Hit(evidence) = read {
        return indexed_standing(path, evidence, references, hash, sources, account, library);
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
                    references,
                    write: IndexWrite::NotAttempted(reason),
                },
                extraction,
                identifier_input: IdentifierInput::NotAttempted(Unattempted::NotAsked),
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
                    references,
                    write: IndexWrite::NotAttempted(reason),
                },
                extraction,
                identifier_input: IdentifierInput::NotAttempted(Unattempted::NotAsked),
                lookup: unheld_lookup(&looked_up, origin, &unresolved, Vec::new()),
                match_check: MatchCheck::NotAttempted(reason),
            };
            return Standing::skipped(SkipReason::Unresolvable, hash, evidence);
        }
    };

    let match_check = title_check(&extraction.titles, &resolved.record);
    let refused = matches!(match_check, MatchCheck::Conflict(_));
    // A refused record writes no evidence for the file's hash: a later
    // run checks the file again rather than being answered for it.
    let write = match (refused, hash.as_ref()) {
        (true, _) => IndexWrite::NotAttempted(Unattempted::Refused),
        (false, Some(hash)) => IndexWrite::Attempted(index.put_extracted(hash, &looked_up)),
        (false, None) => IndexWrite::NotAttempted(Unattempted::Unhashable),
    };
    let file = FileRecord {
        hash: hash.clone(),
        evidence: Evidence {
            library,
            content_index: IndexEvidence {
                read,
                references,
                write,
            },
            extraction,
            identifier_input: IdentifierInput::NotAttempted(Unattempted::NotAsked),
            lookup: found_lookup(looked_up, origin, &resolved, Vec::new()),
            match_check,
        },
        record: resolved.record,
        // Nothing has been overridden: either the conflict check has
        // passed, or this record is refused below and whoever accepts
        // it records that it did.
        overridden: false,
        accepted: false,
        refresh: None,
    };

    if file.conflict().is_some() {
        let evidence = file.evidence.clone();
        return Standing {
            refused: Some(file),
            ..Standing::skipped(SkipReason::Conflict, hash, evidence)
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
    let identifiers = identifiers_of(&file.record);
    match account.and_then(|account| account.work_duplicate(path, file.hash.as_ref(), &identifiers))
    {
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
/// the index read came to, what extraction found and the operator's
/// input stay as `prior` has them.
///
/// A lookup of the file's own identifier made again — an
/// [`Origin::Extracted`] origin over a `prior` whose lookup of the same
/// identifier was attempted — keeps `prior`'s rounds: its `earlier` is
/// `prior`'s earlier rounds followed by `prior`'s current one. An
/// operator's lookup has no earlier rounds. A lookup of the content
/// index's identifier made again is [`resolve_indexed_again`]'s.
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
                references: prior.content_index.references.clone(),
                write: IndexWrite::NotAttempted(Unattempted::AwaitingAcceptance),
            },
            extraction: ExtractionEvidence {
                result: prior.extraction.result.clone(),
                titles,
            },
            identifier_input: prior.identifier_input.clone(),
            lookup: found_lookup(
                identifier.clone(),
                origin,
                &resolved,
                rounds_before(prior, identifier, origin),
            ),
            match_check,
        },
        record: resolved.record,
        // Nothing has been overridden or accepted yet. Accepting this
        // record is the caller's decision, and [`accept`] records it.
        overridden: false,
        accepted: false,
        refresh: None,
    })
}

/// Look up again `identifier`, the identifier the content index
/// answered with for a file, whose evidence has origin `origin`, on top
/// of the file's existing evidence `prior`.
///
/// The same services in the same order as any lookup, with origin
/// [`Origin::Indexed`]. No file is opened and no title check is made,
/// whatever `origin` is: `prior`'s extraction, titles and title check
/// stay as they are, not attempted because the index answered. The
/// content-index write is not attempted for the same reason
/// ([`Unattempted::ContentIndexHit`]), since the index already holds
/// the evidence. What the library said, what the index read came to,
/// and the operator's input stay as `prior` has them.
///
/// The lookup keeps `prior`'s rounds when `prior` looked up the same
/// identifier: its `earlier` is `prior`'s earlier rounds followed by
/// `prior`'s current one.
///
/// The record found is neither overridden nor accepted, and its hash is
/// `None`: the caller, which holds the file's hash, sets it.
///
/// Fails with what every service answered when none holds the
/// identifier; [`unheld_evidence`] gives that lookup its evidence.
pub fn resolve_indexed_again(
    identifier: &Identifier,
    origin: EvidenceOrigin,
    sources: &[&dyn Source],
    prior: &Evidence,
) -> Result<FileRecord, Unresolved> {
    let resolved = from_sources(sources, identifier)?;
    let origin = Origin::Indexed(origin);

    Ok(FileRecord {
        hash: None,
        evidence: Evidence {
            content_index: IndexEvidence {
                read: prior.content_index.read.clone(),
                references: prior.content_index.references.clone(),
                write: IndexWrite::NotAttempted(Unattempted::ContentIndexHit),
            },
            lookup: found_lookup(
                identifier.clone(),
                origin,
                &resolved,
                rounds_before(prior, identifier, origin),
            ),
            ..prior.clone()
        },
        record: resolved.record,
        // The decision the evidence records was made by the run that
        // wrote it, not by this one.
        overridden: false,
        accepted: false,
        refresh: None,
    })
}

/// The evidence of a lookup of `identifier`, which came from `origin`,
/// that no service answered: `prior` with the lookup set to
/// `unresolved`'s attempts, in order, and the title check and the
/// content-index write not attempted because there is no record
/// ([`Unattempted::NoRecord`]). Every other section is `prior`'s.
///
/// For an [`Origin::Indexed`] lookup, the title check and the
/// content-index write stay as `prior` has them instead: neither is
/// made for a lookup of the index's own identifier, record or none.
///
/// The lookup keeps `prior`'s rounds exactly as [`resolve_supplied`]'s
/// and [`resolve_indexed_again`]'s do: only a lookup of the file's own
/// identifier, or of the content index's, made again has earlier
/// rounds.
pub fn unheld_evidence(
    prior: &Evidence,
    identifier: &Identifier,
    origin: Origin,
    unresolved: &Unresolved,
) -> Evidence {
    let lookup = unheld_lookup(
        identifier,
        origin,
        unresolved,
        rounds_before(prior, identifier, origin),
    );
    if let Origin::Indexed(_) = origin {
        return Evidence {
            lookup,
            ..prior.clone()
        };
    }
    let reason = Unattempted::NoRecord;
    Evidence {
        library: prior.library.clone(),
        content_index: IndexEvidence {
            read: prior.content_index.read.clone(),
            references: prior.content_index.references.clone(),
            write: IndexWrite::NotAttempted(reason),
        },
        extraction: prior.extraction.clone(),
        identifier_input: prior.identifier_input.clone(),
        lookup,
        match_check: MatchCheck::NotAttempted(reason),
    }
}

/// The rounds a new lookup of `identifier` from `origin` follows: every
/// round of `prior`'s lookup, oldest first, when `origin` is an
/// extraction pass or the content index and `prior` looked up the same
/// identifier, and none otherwise.
fn rounds_before(prior: &Evidence, identifier: &Identifier, origin: Origin) -> Vec<LookupRound> {
    match (&prior.lookup, origin) {
        (
            LookupEvidence::Attempted {
                identifier: before,
                attempts,
                earlier,
                ..
            },
            Origin::Extracted(_) | Origin::Indexed(_),
        ) if before == identifier => earlier
            .iter()
            .cloned()
            .chain([LookupRound {
                attempts: attempts.clone(),
            }])
            .collect(),
        _ => Vec::new(),
    }
}

/// `file` as an operator's accepting it makes it.
///
/// [`FileRecord::accepted`] is set, and [`FileRecord::overridden`] is
/// set when the title check concluded a conflict, which stays in the
/// evidence as it was. A content-index
/// write held back because the record was
/// refused ([`Unattempted::Refused`]) now waits for the move instead
/// ([`Unattempted::AwaitingAcceptance`]). Nothing else changes: the
/// record, its hash, and the title check's conclusion stay as they
/// were.
pub fn accept(file: FileRecord) -> FileRecord {
    let overridden = file.conflict().is_some();
    let mut accepted = FileRecord {
        overridden,
        accepted: true,
        ..file
    };
    let write = &mut accepted.evidence.content_index.write;
    if *write == IndexWrite::NotAttempted(Unattempted::Refused) {
        *write = IndexWrite::NotAttempted(Unattempted::AwaitingAcceptance);
    }
    accepted
}

/// Keep what identifies `file`'s bytes in the content index, so that
/// a later run recognises them, and say what became of the write.
///
/// [`FileRecord::identification`] is written through the writer for
/// its origin, under `file`'s hash:
///
/// - `supplied` or `overridden` replaces the hash's operator evidence;
/// - `extracted`, which reaches here for a successful retry of the
///   file's own lookup, replaces its extracted evidence.
///
/// Each leaves the other fact as it was. [`IndexWrite::Attempted`]
/// carries the store's result. Nothing is written, and the write is
/// [`IndexWrite::NotAttempted`], for [`Unattempted::Unhashable`] when
/// `file` has no hash; for [`Unattempted::ContentIndexHit`] when the
/// record was reached by a lookup of the index's own identifier, which
/// the index already holds; and, when no lookup reached the record, for
/// the reason the lookup was not made.
///
/// Best-effort, as every write to the response cache is: evidence that
/// cannot be written leaves the file to be asked about next time,
/// which is the safe way to lose an answer, and a failed write is
/// never a failure of the rename it followed.
pub fn remember<S: IndexStore>(index: &ContentIndex<S>, file: &FileRecord) -> IndexWrite {
    let Some(hash) = &file.hash else {
        return IndexWrite::NotAttempted(Unattempted::Unhashable);
    };
    match (&file.evidence.lookup, file.identification()) {
        (LookupEvidence::NotAttempted(reason), _) => IndexWrite::NotAttempted(*reason),
        (
            LookupEvidence::Attempted {
                origin: Origin::Indexed(_),
                ..
            },
            _,
        ) => IndexWrite::NotAttempted(Unattempted::ContentIndexHit),
        (_, Some(evidence)) if evidence.origin.is_operator() => {
            IndexWrite::Attempted(index.put_operator(hash, &evidence))
        }
        (_, Some(evidence)) => {
            IndexWrite::Attempted(index.put_extracted(hash, &evidence.identifier))
        }
        // An attempted lookup always identifies the record it reached.
        (_, None) => IndexWrite::NotAttempted(Unattempted::NoRecord),
    }
}

/// The lookup of `identifier`, which came from `origin`, that reached
/// `resolved`: every service that failed first, then the one that
/// answered, following the rounds `earlier`.
fn found_lookup(
    identifier: Identifier,
    origin: Origin,
    resolved: &Resolved,
    earlier: Vec<LookupRound>,
) -> LookupEvidence {
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
        earlier,
    }
}

/// The lookup of `identifier`, which came from `origin`, that no
/// service answered: `unresolved`'s attempts, in order, following the
/// rounds `earlier`.
fn unheld_lookup(
    identifier: &Identifier,
    origin: Origin,
    unresolved: &Unresolved,
    earlier: Vec<LookupRound>,
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
        earlier,
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

/// The skip an extraction failure reports: each failure its own kind,
/// an unreadable file keeping the reader's message.
fn skipped_for(error: &ExtractionError) -> SkipReason {
    match error {
        ExtractionError::NoTextLayer => SkipReason::NoTextLayer,
        ExtractionError::NoIdentifierFound => SkipReason::TextWithoutIdentifier,
        ExtractionError::Encrypted => SkipReason::Encrypted,
        ExtractionError::Unreadable { message } => SkipReason::Unreadable {
            message: message.clone(),
        },
    }
}

/// What each service answered about an identifier none of them held,
/// one answer per service in the order they were asked.
///
/// For a caller showing a failed lookup rather than reporting it: an
/// identifier the operator supplied and nobody held is a candidate's
/// outcome and not the file's verdict, which carries it only as a
/// submission.
pub fn attempts_of(unresolved: &Unresolved) -> Vec<ServiceAnswer> {
    unresolved
        .attempts
        .iter()
        .map(|(service, error)| {
            ServiceAttempt {
                service: *service,
                outcome: Err(error.clone()),
            }
            .answer()
        })
        .collect()
}

/// The event reporting `standing`'s verdict for `path`: the resolved
/// record's [`resolved_event`], or the skip's [`resolution_skip`]
/// carrying the standing's evidence and, on a conflict, the refused
/// record.
pub fn verdict_event(path: &Path, standing: &Standing) -> Event {
    match &standing.verdict {
        FileOutcome::Resolved(file) => resolved_event(path, file),
        FileOutcome::Skipped(reason) => resolution_skip(
            path,
            reason.clone(),
            &standing.evidence,
            standing.refused.as_ref().map(|file| &file.record),
        ),
    }
}

/// The `skipped` event reporting `reason` as `path`'s resolution
/// verdict, with `evidence`'s sections and acceptance
/// [`Acceptance::NotApplicable`].
///
/// `reason` is meant to be a resolution verdict
/// ([`SkipReason::is_resolution_verdict`]); for any other the event
/// carries no sections, as every skip made after a file resolved
/// carries none. `candidate` is the record the title check refused, and
/// is carried only when `reason` is [`SkipReason::Conflict`].
pub fn resolution_skip(
    path: &Path,
    reason: SkipReason,
    evidence: &Evidence,
    candidate: Option<&Record>,
) -> Event {
    Event::Skipped {
        path: path.to_path_buf(),
        sections: reason
            .is_resolution_verdict()
            .then(|| Box::new(evidence.sections(Acceptance::NotApplicable))),
        candidate: candidate
            .filter(|_| reason == SkipReason::Conflict)
            .map(|record| Box::new(record.clone())),
        reason,
    }
}

/// The event that reports `file` as `path`'s resolution, with its
/// evidence's sections, [`FileRecord::acceptance`], and its refresh's
/// step where it has one.
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
        sections: Box::new(file.evidence.sections(file.acceptance())),
        refresh: file
            .refresh
            .as_ref()
            .map(|refresh| Box::new(refresh.step())),
    }
}

/// The identifier a record is reported under: its DOI, else its arXiv
/// id, else its PMID, else its ISBN, and `None` when the record carries
/// none.
///
/// This is the `resolved` event's `identifier`, and the identifier
/// several library references are compared by.
pub fn preferred_identifier(record: &Record) -> Option<Identifier> {
    record
        .doi
        .clone()
        .map(Identifier::Doi)
        .or_else(|| record.borax.arxiv.clone().map(Identifier::Arxiv))
        .or_else(|| record.pmid.map(Identifier::Pmid))
        .or_else(|| record.isbn.clone().map(Identifier::Isbn))
}

/// [`preferred_identifier`] as the `resolved` event's text, empty when
/// the record carries none.
///
/// Rendered through [`Identifier`]'s own `Display`, so the value
/// carries its kind (`doi:10.1000/xyz`) rather than leaving a consumer
/// to guess which of four kinds a bare string is.
fn identifier_of(record: &Record) -> String {
    preferred_identifier(record)
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
/// into it, so no file's answer depends on another's. The libraries the
/// content index's references name are read once for the whole batch
/// and shared the same way ([`standing_in`]).
///
/// Up to `concurrency` files are resolved at once
/// ([`borax_sources::pace::map_bounded`]). Ordering is unaffected: that
/// helper restores input order whatever order the jobs finish in, so
/// the event stream is the same as a sequential run\'s and a run
/// remains diffable against itself. Pacing is what keeps concurrency
/// from becoming rudeness — it lives in the sources, one interval per
/// service, so however many threads are running no service is asked
/// faster than it allows.
pub fn resolve_batch<S: IndexStore>(
    paths: &[PathBuf],
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<S>,
    library: Option<&Stores>,
    config: &(dyn Fn(&Path) -> ResolveConfig + Sync),
    concurrency: usize,
) -> Run {
    let libraries = Libraries::new();
    let mut events = map_bounded(paths.to_vec(), concurrency, |path| {
        let standing = standing_in(
            &path,
            documents,
            sources,
            index,
            &config(&path),
            None,
            library,
            &libraries,
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
