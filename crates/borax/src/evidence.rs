//! What a resolution found out about a file, step by step.
//!
//! Each step of a resolution — the library, the content index,
//! extraction, the lookup, the title check — either ran and has an
//! answer here, or did not run and says why in one shared vocabulary,
//! [`Unattempted`]. A step that did not run is never recorded as one
//! that ran and found nothing.
//!
//! [`Evidence`] is held by [`crate::pipeline::FileRecord`] and
//! [`crate::pipeline::Standing`], and the fields the schema-3 events
//! carry are derived from it. None of these types is serialised: they
//! describe the engine, not the stream.

use borax_core::identifier::Identifier;
use borax_pdf::tiered::Tier;
use borax_sources::cache::CacheWrite;
use borax_sources::conflict::{Conflict, Insufficient};
use borax_sources::source::{Retrieval, SourceError, SourceName};

use crate::event::{Claim, Extraction, LibraryAnswer};

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
    /// The identifier looked up and every service asked about it.
    pub lookup: LookupEvidence,
    /// What the title check concluded about the record reached.
    pub match_check: MatchCheck,
}

impl Evidence {
    /// The evidence of a verdict reached before any step ran: every
    /// section is not attempted for `reason`, and the library is not
    /// consulted for it.
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
            lookup: LookupEvidence::NotAttempted(reason),
            match_check: MatchCheck::NotAttempted(reason),
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
            return Some(match found.outcome {
                Ok(Retrieval::ServiceCache) => RecordRetrieval::ServiceCache {
                    service: found.service,
                },
                _ => RecordRetrieval::Network {
                    service: found.service,
                },
            });
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

/// The lookup made for a file, or why none was made.
#[derive(Debug, Clone, PartialEq)]
pub enum LookupEvidence {
    /// `identifier`, which came from `origin`, was looked up, and
    /// `attempts` are the services asked, in order. A found attempt is
    /// always the last. No attempts means no configured service
    /// supports the identifier.
    Attempted {
        identifier: Identifier,
        origin: Origin,
        attempts: Vec<ServiceAttempt>,
    },
    /// No lookup was made, for this reason.
    NotAttempted(Unattempted),
}

impl LookupEvidence {
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

    /// Whether the lookup ended in a confirmed absence: there were
    /// attempts and every one was [`SourceError::NotFound`].
    ///
    /// `false` once any service found the record, when any failed to
    /// answer, when no service could be asked, and when no lookup was
    /// made. Only an inconclusive lookup is worth making again.
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
    /// asked about: attempted, with no attempts.
    pub fn no_eligible_service(&self) -> bool {
        matches!(self, LookupEvidence::Attempted { attempts, .. } if attempts.is_empty())
    }

    /// The attempt that found the record, if any.
    fn found(&self) -> Option<&ServiceAttempt> {
        match self {
            LookupEvidence::Attempted { attempts, .. } => {
                attempts.iter().find(|attempt| attempt.outcome.is_ok())
            }
            LookupEvidence::NotAttempted(_) => None,
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

/// One service asked about an identifier, and what it answered.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceAttempt {
    /// The service asked.
    pub service: SourceName,
    /// How it answered: found, with how the answer reached the run, or
    /// the way it failed.
    pub outcome: Result<Retrieval, SourceError>,
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
        }
    }
}
