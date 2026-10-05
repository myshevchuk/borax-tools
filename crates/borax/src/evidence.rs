//! What a resolution found out about a file, step by step.
//!
//! Each step of a resolution — the library, the content index,
//! extraction, an operator's input, the lookup, the title check —
//! either ran and has an answer here, or did not run and says why in
//! one shared vocabulary, [`Unattempted`]. A step that did not run is
//! never recorded as one that ran and found nothing.
//!
//! [`Evidence`] is held by [`crate::pipeline::FileRecord`] and
//! [`crate::pipeline::Standing`]. None of these types is serialised:
//! they describe the engine, not the stream. What a resolution event
//! carries is [`Evidence::sections`], the evidence projected onto the
//! stream's own types in [`crate::event`].

use borax_core::identifier::{Identifier, SuppliedError};
use borax_core::record::Record;
use borax_pdf::tiered::Tier;
use borax_sources::cache::CacheWrite;
use borax_sources::conflict::{Conflict, Insufficient};
use borax_sources::source::{Retrieval, SourceError, SourceName};

use crate::event::{
    Acceptance, Claim, ContentIndexSection, Displaced as DisplacedStep, Extraction,
    ExtractionResultStep, ExtractionSection, FetchedFrom, IdentifierInputStep, IdentifierOrigin,
    IndexReadStep, LibraryAnswer, LibraryStep, LookupRound as LookupRoundStep, LookupStep,
    MatchCheckStep, RetrievedFrom, Sections, ServiceAnswer, ServiceOutcome,
    Submission as SubmissionStep, SubmissionAcceptance, SubmissionOutcome as SubmissionOutcomeStep,
    SyntaxStep, TitlesStep, WriteStep,
};

/// Everything a resolution retained about one file, one section per
/// step, in the order the steps run.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    /// What the run's library said about the file.
    pub library: Consultation,
    /// What the content index did: the read, and the write of the
    /// record reached.
    pub content_index: IndexEvidence,
    /// What extraction found, and the titles the file claims.
    pub extraction: ExtractionEvidence,
    /// Every text an operator typed at the identifier prompt for the
    /// file, or why none was taken.
    pub identifier_input: IdentifierInput,
    /// The identifier looked up and every service asked about it. When
    /// `identifier_input` names a used submission, this is that
    /// submission's lookup.
    pub lookup: LookupEvidence,
    /// What the title check concluded about the record reached.
    pub match_check: MatchCheck,
}

impl Evidence {
    /// The evidence of a verdict reached before any step ran: every
    /// section, the operator's input included, is not attempted for
    /// `reason`, and the library is not consulted for it.
    pub fn not_attempted(reason: Unattempted) -> Evidence {
        Evidence {
            library: Consultation::NotConsulted(reason),
            content_index: IndexEvidence {
                read: IndexRead::NotAttempted(reason),
                write: IndexWrite::NotAttempted(reason),
            },
            extraction: ExtractionEvidence {
                result: ExtractionStep::NotAttempted(reason),
                titles: Titles::NotAttempted(reason),
            },
            identifier_input: IdentifierInput::NotAttempted(reason),
            lookup: LookupEvidence::NotAttempted(reason),
            match_check: MatchCheck::NotAttempted(reason),
        }
    }

    /// This evidence as a resolution event carries it, with
    /// `acceptance` as given.
    ///
    /// Every section maps one to one, and every step that did not run
    /// is not attempted with its reason's [`Unattempted::as_str`] name.
    /// `record_retrieval` is [`Evidence::retrieval`]. A lookup attempted
    /// with no attempts is no eligible service, and the origin carries
    /// no tier, since `extraction`'s result states the pass; each of its
    /// earlier rounds is attempted, or no eligible service when it has
    /// no attempts. A submission's `record_retrieval` is the service of
    /// its lookup's found attempt, or `null`. A found
    /// attempt answered over the network with no response cache in
    /// front of the service reports its `stored` write not attempted
    /// for [`Unattempted::CacheBypassed`]; one answered from the
    /// response cache reports none.
    pub fn sections(&self, acceptance: Acceptance) -> Sections {
        Sections {
            library: match &self.library {
                Consultation::Consulted(answer) => LibraryStep::Consulted {
                    answer: answer.clone(),
                },
                Consultation::NotConsulted(reason) => LibraryStep::NotAttempted {
                    reason: reason.as_str().to_string(),
                },
            },
            content_index: ContentIndexSection {
                read: match &self.content_index.read {
                    IndexRead::Hit => IndexReadStep::Hit,
                    IndexRead::Miss => IndexReadStep::Miss,
                    IndexRead::Bypassed => IndexReadStep::Bypassed,
                    IndexRead::Unavailable { message } => IndexReadStep::Unavailable {
                        message: message.clone(),
                    },
                    IndexRead::NotAttempted(reason) => IndexReadStep::NotAttempted {
                        reason: reason.as_str().to_string(),
                    },
                },
                write: match &self.content_index.write {
                    IndexWrite::Attempted(write) => write_step(write),
                    IndexWrite::NotAttempted(reason) => not_written(*reason),
                },
            },
            extraction: ExtractionSection {
                result: match &self.extraction.result {
                    ExtractionStep::Ran(extraction) => match extraction {
                        Extraction::Found { identifier, tier } => ExtractionResultStep::Found {
                            identifier: identifier.clone(),
                            tier: tier.clone(),
                        },
                        Extraction::NoTextLayer => ExtractionResultStep::NoTextLayer,
                        Extraction::TextWithoutIdentifier => {
                            ExtractionResultStep::TextWithoutIdentifier
                        }
                        Extraction::Encrypted => ExtractionResultStep::Encrypted,
                        Extraction::Unreadable { message } => ExtractionResultStep::Unreadable {
                            message: message.clone(),
                        },
                    },
                    ExtractionStep::NotAttempted(reason) => ExtractionResultStep::NotAttempted {
                        reason: reason.as_str().to_string(),
                    },
                },
                titles: match &self.extraction.titles {
                    Titles::Read(claims) => TitlesStep::Read {
                        claims: claims.clone(),
                    },
                    Titles::Failed { message } => TitlesStep::Failed {
                        message: message.clone(),
                    },
                    Titles::NotAttempted(reason) => TitlesStep::NotAttempted {
                        reason: reason.as_str().to_string(),
                    },
                },
            },
            identifier_input: self.identifier_input.step(),
            lookup: self.lookup.step(),
            record_retrieval: self.retrieval().map(RecordRetrieval::retrieved_from),
            match_check: self.match_check.step(),
            acceptance,
        }
    }

    /// Where the record this evidence is about came from: the first of
    /// these that holds.
    ///
    /// 1. The lookup has an attempt that found the record: that
    ///    service, from its response cache or over the network.
    /// 2. The lookup was not attempted because the library answered,
    ///    and the library tracks the file: its artifact and item.
    /// 3. The content index answered.
    ///
    /// `None` when none holds, which is evidence of no record.
    ///
    /// The lookup is asked first because an operator's lookup can sit
    /// on evidence that also records a library answer or an index hit,
    /// and the record it reached came from the service.
    pub fn retrieval(&self) -> Option<RecordRetrieval> {
        if let Some(found) = self.lookup.found() {
            return Some(RecordRetrieval::from_attempt(found));
        }

        if let (
            LookupEvidence::NotAttempted(Unattempted::LibraryAnswered),
            Consultation::Consulted(LibraryAnswer::Tracked { artifact, item }),
        ) = (&self.lookup, &self.library)
        {
            return Some(RecordRetrieval::Library {
                artifact: artifact.clone(),
                item: item.clone(),
            });
        }

        match self.content_index.read {
            IndexRead::Hit => Some(RecordRetrieval::ContentIndex),
            _ => None,
        }
    }
}

/// What the run's library said about a file, or why it was not asked.
#[derive(Debug, Clone, PartialEq)]
pub enum Consultation {
    /// The library was asked and gave this answer.
    Consulted(LibraryAnswer),
    /// The library was not asked, for this reason.
    NotConsulted(Unattempted),
}

impl Consultation {
    /// The library's answer, or `None` when it was not asked.
    pub fn answer(&self) -> Option<&LibraryAnswer> {
        match self {
            Consultation::Consulted(answer) => Some(answer),
            Consultation::NotConsulted(_) => None,
        }
    }
}

/// What the content index did for a file.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexEvidence {
    /// Whether the index answered for the file's hash.
    pub read: IndexRead,
    /// What became of writing the record reached under the file's hash.
    pub write: IndexWrite,
}

/// What reading the content index came to.
#[derive(Debug, Clone, PartialEq)]
pub enum IndexRead {
    /// The index held a record for the file's hash.
    Hit,
    /// The index held nothing for the file's hash.
    Miss,
    /// The run turned the cache off, so the index was not read.
    Bypassed,
    /// The file could not be hashed, so there was nothing to read the
    /// index by. `message` is the hashing error's. Reported even when
    /// the run turned the cache off.
    Unavailable { message: String },
    /// The index was not read, for this reason.
    NotAttempted(Unattempted),
}

/// What writing the record reached to the content index came to.
#[derive(Debug, Clone, PartialEq)]
pub enum IndexWrite {
    /// The write was made, with the store's result.
    Attempted(CacheWrite),
    /// No write was made, for the earliest reason in pipeline order.
    NotAttempted(Unattempted),
}

/// What extraction found in a file, and the titles the file claims.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionEvidence {
    /// The extractor's result, one of the five kinds
    /// [`crate::event::Extraction`] tells apart.
    pub result: ExtractionStep,
    /// The titles the file claims, held apart from `result`.
    pub titles: Titles,
}

/// Whether extraction ran, and what it found.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractionStep {
    /// Extraction ran over the file with this result.
    Ran(Extraction),
    /// Extraction did not run, for this reason.
    NotAttempted(Unattempted),
}

/// The titles a file claims, in exactly one of three states.
#[derive(Debug, Clone, PartialEq)]
pub enum Titles {
    /// The file was opened, and these are every title it claims, in the
    /// order read. Empty when it claims none.
    Read(Vec<Claim>),
    /// The file could not be opened. `message` is the reason.
    Failed { message: String },
    /// The file was not opened, for this reason.
    NotAttempted(Unattempted),
}

impl Titles {
    /// The titles read, and none for titles that failed or were not
    /// attempted.
    pub fn claims(&self) -> &[Claim] {
        match self {
            Titles::Read(claims) => claims,
            Titles::Failed { .. } | Titles::NotAttempted(_) => &[],
        }
    }
}

/// What an operator typed at the identifier prompt for a file.
// Held once per file, inside evidence that is itself cloned whole; a
// box would put an allocation on every supplied file to shrink a value
// nothing keeps in bulk.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum IdentifierInput {
    /// At least one text was submitted. `submissions` are every one, in
    /// the order typed and numbered from 1. `used` is `Some` exactly
    /// when the evidence's own lookup is a submission's: it names that
    /// submission, whose `outcome` is then `None`, and holds the file's
    /// own steps its outcome displaced.
    Supplied {
        submissions: Vec<Submission>,
        used: Option<Displacement>,
    },
    /// Nothing was submitted, for this reason: `NotAsked`,
    /// `NotSupplied` or `ContentDuplicate`.
    NotAttempted(Unattempted),
}

impl IdentifierInput {
    /// This input as a resolution event carries it.
    fn step(&self) -> IdentifierInputStep {
        match self {
            IdentifierInput::Supplied { submissions, used } => IdentifierInputStep::Supplied {
                submissions: submissions.iter().map(Submission::step).collect(),
                used: used.as_ref().map(|used| used.submission),
                displaced: used.as_ref().map(|used| Box::new(used.displaced.step())),
            },
            IdentifierInput::NotAttempted(reason) => IdentifierInputStep::NotAttempted {
                reason: reason.as_str().to_string(),
            },
        }
    }
}

/// One text an operator submitted at the identifier prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Submission {
    /// Its number, counting from 1 within the file.
    pub number: u32,
    /// The text as the prompt returned it.
    pub raw: String,
    /// The identifier it parsed to, or why it named none.
    pub syntax: Result<Identifier, SuppliedError>,
    /// What it came to. `None` exactly for the submission `used` names,
    /// whose outcome is the evidence's own sections.
    pub outcome: Option<SubmissionOutcome>,
}

impl Submission {
    /// This submission as a resolution event carries it.
    fn step(&self) -> SubmissionStep {
        SubmissionStep {
            submission: self.number,
            raw: self.raw.clone(),
            syntax: match &self.syntax {
                Ok(identifier) => SyntaxStep::Parsed {
                    identifier: identifier.to_string(),
                },
                Err(error) => SyntaxStep::Rejected {
                    reason: error.reason().to_string(),
                    expected: error.expected().map(|kind| kind.as_str().to_string()),
                },
            },
            outcome: self.outcome.as_ref().map(|outcome| {
                Box::new(SubmissionOutcomeStep {
                    lookup: outcome.lookup.step(),
                    record_retrieval: outcome
                        .lookup
                        .found()
                        .map(|found| RecordRetrieval::from_attempt(found).retrieved_from()),
                    match_check: outcome.match_check.step(),
                    acceptance: match outcome.decision {
                        CandidateDecision::Rejected => SubmissionAcceptance::Rejected,
                        CandidateDecision::NotAttempted(reason) => {
                            SubmissionAcceptance::NotAttempted {
                                reason: reason.as_str().to_string(),
                            }
                        }
                    },
                    record: outcome.record.clone().map(Box::new),
                })
            }),
        }
    }
}

/// What a submission that was not used came to.
#[derive(Debug, Clone, PartialEq)]
pub struct SubmissionOutcome {
    /// The lookup of the identifier it parsed to, with
    /// [`Origin::Operator`], or not attempted for
    /// [`Unattempted::Unparsed`].
    pub lookup: LookupEvidence,
    /// What the title check concluded about its record.
    pub match_check: MatchCheck,
    /// The record it reached, if any.
    pub record: Option<Record>,
    /// What the operator decided about that record.
    pub decision: CandidateDecision,
}

/// What an operator decided about the record a submission reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateDecision {
    /// Its record was on offer and stopped being: the operator answered
    /// Skip, or a later submission's record took its place.
    Rejected,
    /// It reached nothing to decide about, for this reason:
    /// [`Unattempted::Unparsed`], [`Unattempted::NoRecord`] or
    /// [`Unattempted::NoMove`].
    NotAttempted(Unattempted),
}

/// The submission whose record the evidence reports, and the file's
/// own steps that record displaced.
#[derive(Debug, Clone, PartialEq)]
pub struct Displacement {
    /// The used submission's number.
    pub submission: u32,
    /// The file's own steps the used submission's outcome replaced.
    pub displaced: Displaced,
}

/// The file's own lookup, retrieval, title check and record, as they
/// stood before an operator's submission replaced them.
#[derive(Debug, Clone, PartialEq)]
pub struct Displaced {
    /// The file's own lookup, every round of it.
    pub lookup: LookupEvidence,
    /// Where the file's own record came from, or `None` when its own
    /// resolution reached none.
    pub retrieval: Option<RecordRetrieval>,
    /// The file's own title check.
    pub match_check: MatchCheck,
    /// The file's own record, resolved or refused.
    pub record: Option<Record>,
}

impl Displaced {
    /// These facts as a resolution event carries them.
    fn step(&self) -> DisplacedStep {
        DisplacedStep {
            lookup: self.lookup.step(),
            record_retrieval: self.retrieval.clone().map(RecordRetrieval::retrieved_from),
            match_check: self.match_check.step(),
            record: self.record.clone().map(Box::new),
        }
    }
}

/// The lookup made for a file, or why none was made.
///
/// The variant and its fields describe the current round, the one that
/// decides what the lookup found and whether it is conclusive.
#[derive(Debug, Clone, PartialEq)]
pub enum LookupEvidence {
    /// `identifier`, which came from `origin`, was looked up, and
    /// `attempts` are the services asked, in order. A found attempt is
    /// always the last. No attempts means no configured service
    /// supports the identifier. `earlier` holds the rounds of the same
    /// lookup made before this one, oldest first, when an operator
    /// asked for the file's own identifier to be looked up again; it is
    /// empty otherwise.
    Attempted {
        identifier: Identifier,
        origin: Origin,
        attempts: Vec<ServiceAttempt>,
        earlier: Vec<LookupRound>,
    },
    /// No lookup was made, for this reason.
    NotAttempted(Unattempted),
}

/// One earlier round of a file's own lookup; no attempts means no
/// configured service supported it.
#[derive(Debug, Clone, PartialEq)]
pub struct LookupRound {
    /// The services asked in that round, in order.
    pub attempts: Vec<ServiceAttempt>,
}

impl LookupEvidence {
    /// This lookup as a resolution event carries it.
    fn step(&self) -> LookupStep {
        match self {
            LookupEvidence::Attempted {
                identifier,
                origin,
                attempts,
                earlier,
            } if attempts.is_empty() => LookupStep::NoEligibleService {
                identifier: identifier.to_string(),
                origin: origin.identifier_origin(),
                earlier: earlier.iter().map(LookupRound::step).collect(),
            },
            LookupEvidence::Attempted {
                identifier,
                origin,
                attempts,
                earlier,
            } => LookupStep::Attempted {
                identifier: identifier.to_string(),
                origin: origin.identifier_origin(),
                attempts: attempts.iter().map(ServiceAttempt::answer).collect(),
                earlier: earlier.iter().map(LookupRound::step).collect(),
            },
            LookupEvidence::NotAttempted(reason) => LookupStep::NotAttempted {
                reason: reason.as_str().to_string(),
            },
        }
    }

    /// The identifier looked up and the extraction pass that read it,
    /// or `None` when nothing was looked up or an operator supplied the
    /// identifier.
    pub fn extracted(&self) -> Option<(&Identifier, Tier)> {
        match self {
            LookupEvidence::Attempted {
                identifier,
                origin: Origin::Extracted(tier),
                ..
            } => Some((identifier, *tier)),
            _ => None,
        }
    }

    /// Whether the current round ended in a confirmed absence: there
    /// were attempts and every one was [`SourceError::NotFound`].
    /// Earlier rounds play no part.
    ///
    /// `false` once any service found the record, when any failed to
    /// answer, when no service could be asked, and when no lookup was
    /// made. Only a lookup whose current round is inconclusive is worth
    /// making again.
    pub fn is_conclusive(&self) -> bool {
        match self {
            LookupEvidence::Attempted { attempts, .. } => {
                !attempts.is_empty()
                    && attempts
                        .iter()
                        .all(|attempt| matches!(attempt.outcome, Err(SourceError::NotFound)))
            }
            LookupEvidence::NotAttempted(_) => false,
        }
    }

    /// Whether a lookup was made that no configured service could be
    /// asked about in its current round: attempted, with no attempts.
    pub fn no_eligible_service(&self) -> bool {
        matches!(self, LookupEvidence::Attempted { attempts, .. } if attempts.is_empty())
    }

    /// The current round's attempt that found the record, if any.
    fn found(&self) -> Option<&ServiceAttempt> {
        match self {
            LookupEvidence::Attempted { attempts, .. } => {
                attempts.iter().find(|attempt| attempt.outcome.is_ok())
            }
            LookupEvidence::NotAttempted(_) => None,
        }
    }
}

impl LookupRound {
    /// This round as a resolution event carries it.
    fn step(&self) -> LookupRoundStep {
        match self.attempts.is_empty() {
            true => LookupRoundStep::NoEligibleService,
            false => LookupRoundStep::Attempted {
                attempts: self.attempts.iter().map(ServiceAttempt::answer).collect(),
            },
        }
    }
}

/// Where an identifier that was looked up came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// An extraction pass read it from the file, including when an
    /// operator asked for the lookup to be made again.
    Extracted(Tier),
    /// An operator supplied it.
    Operator,
}

impl Origin {
    /// The origin as a resolution event reports it, without the pass,
    /// which the event's extraction result states.
    pub(crate) fn identifier_origin(self) -> IdentifierOrigin {
        match self {
            Origin::Extracted(_) => IdentifierOrigin::Extracted,
            Origin::Operator => IdentifierOrigin::Operator,
        }
    }
}

/// One service asked about an identifier, and what it answered.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceAttempt {
    /// The service asked.
    pub service: SourceName,
    /// How it answered: found, with how the answer reached the run, or
    /// the way it failed.
    pub outcome: Result<Retrieval, SourceError>,
}

impl ServiceAttempt {
    /// This attempt as a resolution event reports it.
    pub(crate) fn answer(&self) -> ServiceAnswer {
        ServiceAnswer {
            service: self.service.as_str().to_string(),
            outcome: match &self.outcome {
                Ok(Retrieval::ServiceCache) => ServiceOutcome::Found {
                    retrieval: FetchedFrom::ServiceCache,
                    stored: None,
                },
                Ok(Retrieval::Network { stored }) => ServiceOutcome::Found {
                    retrieval: FetchedFrom::Network,
                    stored: Some(match stored {
                        Some(write) => write_step(write),
                        None => not_written(Unattempted::CacheBypassed),
                    }),
                },
                Err(SourceError::NotFound) => ServiceOutcome::NotFound,
                Err(SourceError::Unavailable { message }) => ServiceOutcome::Unavailable {
                    message: message.clone(),
                },
                Err(SourceError::RateLimited) => ServiceOutcome::RateLimited,
                Err(SourceError::Malformed { message }) => ServiceOutcome::Malformed {
                    message: message.clone(),
                },
            },
        }
    }
}

/// `write` as the stream reports a write that was made.
pub(crate) fn write_step(write: &CacheWrite) -> WriteStep {
    match write {
        CacheWrite::Written => WriteStep::Written,
        CacheWrite::Failed { message } => WriteStep::Failed {
            message: message.clone(),
        },
    }
}

/// A write not made, for `reason`.
fn not_written(reason: Unattempted) -> WriteStep {
    WriteStep::NotAttempted {
        reason: reason.as_str().to_string(),
    }
}

/// Where the record a resolution reached was retrieved from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordRetrieval {
    /// The file's library item, through the artifact record `artifact`.
    Library { artifact: String, item: String },
    /// The content index, under the file's hash.
    ContentIndex,
    /// `service`'s response cache. No request was sent.
    ServiceCache { service: SourceName },
    /// `service`, over the network.
    Network { service: SourceName },
}

impl RecordRetrieval {
    /// Where `found`, an attempt that found its record, retrieved it
    /// from: the service's response cache, or the network.
    fn from_attempt(found: &ServiceAttempt) -> RecordRetrieval {
        match found.outcome {
            Ok(Retrieval::ServiceCache) => RecordRetrieval::ServiceCache {
                service: found.service,
            },
            _ => RecordRetrieval::Network {
                service: found.service,
            },
        }
    }

    /// This retrieval as a resolution event carries it.
    fn retrieved_from(self) -> RetrievedFrom {
        match self {
            RecordRetrieval::Library { artifact, item } => {
                RetrievedFrom::Library { artifact, item }
            }
            RecordRetrieval::ContentIndex => RetrievedFrom::ContentIndex,
            RecordRetrieval::ServiceCache { service } => RetrievedFrom::ServiceCache {
                service: service.as_str().to_string(),
            },
            RecordRetrieval::Network { service } => RetrievedFrom::Network {
                service: service.as_str().to_string(),
            },
        }
    }
}

/// What the title check concluded about the record reached.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchCheck {
    /// A title the file claims names the record's work.
    Agreed,
    /// The titles the file claims name another work.
    Conflict(Conflict),
    /// There was too little evidence to judge, for this reason.
    Insufficient(Insufficient),
    /// The check was not made, for this reason.
    NotAttempted(Unattempted),
}

impl MatchCheck {
    /// This check as a resolution event carries it.
    fn step(&self) -> MatchCheckStep {
        match self {
            MatchCheck::Agreed => MatchCheckStep::Agreed,
            MatchCheck::Conflict(conflict) => MatchCheckStep::Conflict {
                field: conflict.field.to_string(),
                extracted: conflict.extracted.clone(),
                resolved: conflict.resolved.clone(),
                similarity: conflict.similarity,
            },
            MatchCheck::Insufficient(insufficient) => MatchCheckStep::InsufficientEvidence {
                reason: match insufficient {
                    Insufficient::RecordUntitled => "record-untitled",
                    Insufficient::NoTitles => "no-titles",
                    Insufficient::NoEvidence => "no-evidence",
                }
                .to_string(),
            },
            MatchCheck::NotAttempted(reason) => MatchCheckStep::NotAttempted {
                reason: reason.as_str().to_string(),
            },
        }
    }
}

/// Why a step of a resolution was not taken.
///
/// One vocabulary for every step; which reasons a step can carry
/// follows from where it sits in the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unattempted {
    /// The run has no library.
    NoLibrary,
    /// The file lies outside the run's library, or in a subtree it
    /// excludes.
    OutsideLibrary,
    /// The library already holds the file's bytes.
    ContentDuplicate,
    /// The library answered with the file's item.
    LibraryAnswered,
    /// The content index answered for the file.
    ContentIndexHit,
    /// Extraction found no identifier.
    ExtractionFailed,
    /// No service held the identifier, or none could be asked.
    NoRecord,
    /// The title check refused the record.
    Refused,
    /// The file has no content hash.
    Unhashable,
    /// The record waits for an operator to accept it, and is written
    /// when the file is moved.
    AwaitingAcceptance,
    /// No response cache stood in front of the service that answered,
    /// so its answer was not stored. Only [`Evidence::sections`] gives
    /// this reason, for a network answer's `stored` write; no section
    /// of the evidence itself holds it.
    CacheBypassed,
    /// No question was put to an operator about the file.
    NotAsked,
    /// An operator was asked about the file and submitted no text.
    NotSupplied,
    /// The text an operator submitted named no identifier.
    Unparsed,
    /// The record a submission reached leads to no move: its target is
    /// taken, it renders no name, or it renders the file's current
    /// name.
    NoMove,
}

impl Unattempted {
    /// The reason's kebab-case name.
    pub fn as_str(self) -> &'static str {
        match self {
            Unattempted::NoLibrary => "no-library",
            Unattempted::OutsideLibrary => "outside-library",
            Unattempted::ContentDuplicate => "content-duplicate",
            Unattempted::LibraryAnswered => "library-answered",
            Unattempted::ContentIndexHit => "content-index-hit",
            Unattempted::ExtractionFailed => "extraction-failed",
            Unattempted::NoRecord => "no-record",
            Unattempted::Refused => "refused",
            Unattempted::Unhashable => "unhashable",
            Unattempted::AwaitingAcceptance => "awaiting-acceptance",
            Unattempted::CacheBypassed => "cache-bypassed",
            Unattempted::NotAsked => "not-asked",
            Unattempted::NotSupplied => "not-supplied",
            Unattempted::Unparsed => "unparsed",
            Unattempted::NoMove => "no-move",
        }
    }
}
