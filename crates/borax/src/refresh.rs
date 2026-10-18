//! A fresh lookup of the record a library item answers a file with.
//!
//! A run given `--refresh` looks an item's preferred identifier up
//! again. What the services return is a candidate: it is compared with
//! the item's record field by field ([`differences`]) and reported
//! beside it, and the item's record stays the file's. Replacing the
//! item's record with it ([`replaced`]) is a separate act, which only an
//! operator's answer makes.

use std::path::Path;

use borax_core::identifier::Identifier;
use borax_core::index::LibraryReference;
use borax_core::library::ItemId;
use borax_core::record::Record;
use borax_sources::dispatch;
use borax_sources::source::Source;
use serde_json::Value;

use crate::event::{DifferenceStep, RefreshAcceptance, RefreshStep};
use crate::evidence::{ServiceAttempt, Unattempted};
use crate::library::Stores;
use crate::paths::lexical;
use crate::pipeline::preferred_identifier;

/// A field a refresh compares, named by the key [`Record`] serializes it
/// under.
///
/// Provenance, confidence and source fields are not among them: they say
/// where a value came from and how a service files the work, not what the
/// work is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Type,
    Title,
    Author,
    Issued,
    ContainerTitle,
    Volume,
    Issue,
    Page,
    Publisher,
    Doi,
    Pmid,
    Isbn,
    Arxiv,
}

impl Field {
    /// Every compared field, in the order differences are reported in.
    pub const ALL: [Field; 13] = [
        Field::Type,
        Field::Title,
        Field::Author,
        Field::Issued,
        Field::ContainerTitle,
        Field::Volume,
        Field::Issue,
        Field::Page,
        Field::Publisher,
        Field::Doi,
        Field::Pmid,
        Field::Isbn,
        Field::Arxiv,
    ];

    /// The record key: `type`, `title`, `author`, `issued`,
    /// `container-title`, `volume`, `issue`, `page`, `publisher`, `DOI`,
    /// `PMID`, `ISBN`, and `arxiv`, which sits under `borax`. It is also
    /// the key of the field's provenance entry.
    pub fn as_str(self) -> &'static str {
        match self {
            Field::Type => "type",
            Field::Title => "title",
            Field::Author => "author",
            Field::Issued => "issued",
            Field::ContainerTitle => "container-title",
            Field::Volume => "volume",
            Field::Issue => "issue",
            Field::Page => "page",
            Field::Publisher => "publisher",
            Field::Doi => "DOI",
            Field::Pmid => "PMID",
            Field::Isbn => "ISBN",
            Field::Arxiv => "arxiv",
        }
    }

    /// The field's value as `record` serializes it; `None` where the
    /// record has none.
    pub fn value(self, record: &Record) -> Option<Value> {
        self.value_in(&serialized(record))
    }

    /// The field's value in `record`, a [`Record`] serialized.
    fn value_in(self, record: &Value) -> Option<Value> {
        let value = match self {
            Field::Arxiv => record.get("borax")?.get(self.as_str())?,
            _ => record.get(self.as_str())?,
        };
        (!value.is_null()).then(|| value.clone())
    }

    /// Set this field of `to` to its value in `from`, absent where
    /// `from` has none.
    fn copy(self, from: &Record, to: &mut Record) {
        match self {
            Field::Type => to.entry_type = from.entry_type,
            Field::Title => to.title = from.title.clone(),
            Field::Author => to.authors = from.authors.clone(),
            Field::Issued => to.issued = from.issued,
            Field::ContainerTitle => to.container_title = from.container_title.clone(),
            Field::Volume => to.volume = from.volume.clone(),
            Field::Issue => to.issue = from.issue.clone(),
            Field::Page => to.pages = from.pages.clone(),
            Field::Publisher => to.publisher = from.publisher.clone(),
            Field::Doi => to.doi = from.doi.clone(),
            Field::Pmid => to.pmid = from.pmid,
            Field::Isbn => to.isbn = from.isbn.clone(),
            Field::Arxiv => to.borax.arxiv = from.borax.arxiv.clone(),
        }
    }
}

/// `record` as JSON. A record always serializes, being plain data with
/// string keys; `Null` stands in for the failure that cannot happen.
fn serialized(record: &Record) -> Value {
    serde_json::to_value(record).unwrap_or(Value::Null)
}

/// One compared field whose value differs between an item's record and
/// a candidate: each side's value as [`Field::value`] gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    pub field: Field,
    pub item: Option<Value>,
    pub candidate: Option<Value>,
}

impl Difference {
    /// This difference as the `refresh` key reports it.
    fn step(&self) -> DifferenceStep {
        DifferenceStep {
            field: self.field.as_str().to_string(),
            item: self.item.clone(),
            candidate: self.candidate.clone(),
        }
    }
}

/// Every compared field whose value differs between `item` and
/// `candidate`, in [`Field::ALL`] order; empty when they agree in every
/// one.
///
/// Values are equal when their JSON is, an absent value included, and
/// nothing is normalised first: whitespace and case count, authors are
/// compared in order, and a date to its precision.
pub fn differences(item: &Record, candidate: &Record) -> Vec<Difference> {
    let (item, candidate) = (serialized(item), serialized(candidate));
    Field::ALL
        .into_iter()
        .filter_map(|field| {
            let (item, candidate) = (field.value_in(&item), field.value_in(&candidate));
            (item != candidate).then_some(Difference {
                field,
                item,
                candidate,
            })
        })
        .collect()
}

/// The record that replacing `item`'s with `candidate` writes: `item`,
/// with every field [`differences`] names, and that field's provenance
/// entry, taken from `candidate`, set or removed as `candidate` has it.
///
/// Everything else is `item`'s own: its other provenance entries, its
/// source fields and its confidence. So the result has no difference
/// from `candidate`.
pub fn replaced(item: &Record, candidate: &Record) -> Record {
    let mut record = item.clone();
    for Difference { field, .. } in differences(item, candidate) {
        field.copy(candidate, &mut record);
        let key = field.as_str();
        match candidate.borax.provenance.get(key) {
            Some(source) => record.borax.provenance.insert(key.to_string(), *source),
            None => record.borax.provenance.remove(key),
        };
    }
    record
}

/// What a fresh lookup of an item's preferred identifier found.
// One of these is made per refreshed file and moved into its verdict
// once, so the bytes the large variant costs are paid once per file.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Fresh {
    /// The item's record carries no identifier, so nothing was asked.
    NoIdentifier,
    /// No service given supports `identifier`, so none was asked.
    NoEligibleService { identifier: Identifier },
    /// Every service asked about `identifier` failed, each in
    /// `attempts`, in the order asked.
    Unresolved {
        identifier: Identifier,
        attempts: Vec<ServiceAttempt>,
    },
    /// The record found equals the item's in every compared field.
    /// `attempts` ends with the service that answered.
    Current {
        identifier: Identifier,
        attempts: Vec<ServiceAttempt>,
    },
    /// The record found, `candidate`, differs from the item's in each of
    /// `differences`. `attempts` ends with the service that answered.
    Differs {
        identifier: Identifier,
        attempts: Vec<ServiceAttempt>,
        candidate: Record,
        differences: Vec<Difference>,
    },
}

/// What became of a refresh's candidate.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Nobody was asked, for the reason given.
    NotAttempted(Unattempted),
    /// An operator is being asked.
    Pending,
    /// The operator replaced the item's record, and the write landed.
    Accepted,
    /// The operator chose to replace the item's record, and the write
    /// failed or was refused for `message`.
    Unwritten { message: String },
    /// The operator kept the item's record.
    Declined,
}

impl Decision {
    /// This decision as the `refresh` key's `acceptance` reports it.
    fn acceptance(&self) -> RefreshAcceptance {
        match self {
            Decision::NotAttempted(reason) => RefreshAcceptance::NotAttempted {
                reason: reason.as_str().to_string(),
            },
            Decision::Pending => RefreshAcceptance::Pending,
            Decision::Accepted => RefreshAcceptance::Accepted,
            Decision::Unwritten { message } => RefreshAcceptance::Unwritten {
                message: message.clone(),
            },
            Decision::Declined => RefreshAcceptance::Declined,
        }
    }
}

/// Whose item a refresh was of.
///
/// `Own`: the run's library answered, or a reference whose `library` is
/// the run's library's identity and whose root, normalised lexically, is
/// the run's library root. `Referenced`: every other reference answer,
/// another identity or the same identity at another root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whose {
    Own,
    Referenced,
}

/// Whose item `reference` names, for a run whose library is `library`
/// (`None` for a run in no library).
///
/// [`Whose::Own`] only where the library's identity was present when its
/// stores were read ([`Stores::identity`]), equals the reference's, and
/// the reference's root, normalised lexically, is [`Stores::root`].
/// Asks nothing of the filesystem.
pub fn whose(reference: &LibraryReference, library: Option<&Stores>) -> Whose {
    let root = |path: &Path| lexical(path).unwrap_or_else(|| path.to_path_buf());
    match library {
        Some(stores)
            if stores.identity() == Some(&reference.library)
                && root(&reference.root) == stores.root() =>
        {
            Whose::Own
        }
        _ => Whose::Referenced,
    }
}

/// A refresh of the item a file resolved from: which item, whose it is,
/// what the fresh lookup found, and what became of its candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct Refresh {
    pub item: ItemId,
    pub whose: Whose,
    pub fresh: Fresh,
    pub decision: Decision,
}

impl Refresh {
    /// The `refresh` key a `resolved` event carries. `decision` is
    /// reported, as the step's `acceptance`, only for [`Fresh::Differs`].
    pub fn step(&self) -> RefreshStep {
        let answers =
            |attempts: &[ServiceAttempt]| attempts.iter().map(ServiceAttempt::answer).collect();
        match &self.fresh {
            Fresh::NoIdentifier => RefreshStep::NotAttempted {
                reason: Unattempted::NoIdentifier.as_str().to_string(),
            },
            Fresh::NoEligibleService { identifier } => RefreshStep::NoEligibleService {
                identifier: identifier.to_string(),
            },
            Fresh::Unresolved {
                identifier,
                attempts,
            } => RefreshStep::Unresolved {
                identifier: identifier.to_string(),
                attempts: answers(attempts),
            },
            Fresh::Current {
                identifier,
                attempts,
            } => RefreshStep::Current {
                identifier: identifier.to_string(),
                attempts: answers(attempts),
            },
            Fresh::Differs {
                identifier,
                attempts,
                candidate,
                differences,
            } => RefreshStep::Differs {
                identifier: identifier.to_string(),
                attempts: answers(attempts),
                differences: differences.iter().map(Difference::step).collect(),
                candidate: Box::new(candidate.clone()),
                acceptance: self.decision.acceptance(),
            },
        }
    }
}

/// Look `record`'s preferred identifier ([`preferred_identifier`]) up
/// over `sources`, in [`dispatch::resolve`] order, and compare what is
/// found with `record`.
///
/// Asks nothing for a record with no identifier. Every service asked is
/// in the result's attempts, in the order asked, a failure before the
/// service that answered included. Never fails: a lookup nobody answered
/// is [`Fresh::Unresolved`], and one nobody could be asked is
/// [`Fresh::NoEligibleService`].
pub fn refresh(record: &Record, sources: &[&dyn Source]) -> Fresh {
    let Some(identifier) = preferred_identifier(record) else {
        return Fresh::NoIdentifier;
    };
    let failed = |failures: Vec<(_, _)>| {
        failures
            .into_iter()
            .map(|(service, error)| ServiceAttempt {
                service,
                outcome: Err(error),
            })
            .collect::<Vec<_>>()
    };
    match dispatch::resolve(sources, &identifier) {
        Err(unresolved) if unresolved.attempts.is_empty() => {
            Fresh::NoEligibleService { identifier }
        }
        Err(unresolved) => Fresh::Unresolved {
            identifier,
            attempts: failed(unresolved.attempts),
        },
        Ok(resolved) => {
            let mut attempts = failed(resolved.failures);
            attempts.push(ServiceAttempt {
                service: resolved.source,
                outcome: Ok(resolved.retrieval),
            });
            let differences = differences(record, &resolved.record);
            match differences.is_empty() {
                true => Fresh::Current {
                    identifier,
                    attempts,
                },
                false => Fresh::Differs {
                    identifier,
                    attempts,
                    candidate: resolved.record,
                    differences,
                },
            }
        }
    }
}
