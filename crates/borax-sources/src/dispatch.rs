//! Choosing which source to ask, and in what order.

use borax_core::identifier::Identifier;
use borax_core::record::Record;

use crate::source::{Source, SourceError, SourceName};

/// A record and the source that supplied it.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub record: Record,
    pub source: SourceName,
}

/// Nobody could answer. `attempts` lists what each consulted source
/// said, in the order they were asked, so the run summary can explain
/// itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub attempts: Vec<(SourceName, SourceError)>,
}

impl Unresolved {
    /// Whether every source that answered said it simply does not hold
    /// the identifier — as opposed to being unreachable, rate-limited,
    /// or unreadable. A genuinely unknown identifier is a different
    /// report from a failed lookup, and only the latter is worth
    /// retrying.
    pub fn is_conclusive(&self) -> bool {
        !self.attempts.is_empty()
            && self
                .attempts
                .iter()
                .all(|(_, error)| matches!(error, SourceError::NotFound))
    }
}

/// The order to consult sources for an identifier.
///
/// - A DOI goes to Crossref first (the registry of record), then
///   OpenAlex, then DataCite (which registers the DOIs Crossref does
///   not: theses, datasets, many institutional repositories).
/// - An arXiv identifier goes to arXiv first, then OpenAlex, which
///   indexes preprints and may already know the published version.
/// - A PMID goes to PubMed, then OpenAlex.
/// - An ISBN goes to OpenAlex (Crossref's book coverage is
///   registrant-dependent and DataCite has none).
///
/// The list is a preference, not a guarantee: [`resolve`] skips names
/// it was not given.
pub fn priority(identifier: &Identifier) -> Vec<SourceName> {
    match identifier {
        Identifier::Doi(_) => vec![
            SourceName::Crossref,
            SourceName::OpenAlex,
            SourceName::DataCite,
        ],
        Identifier::Arxiv(_) => vec![SourceName::Arxiv, SourceName::OpenAlex],
        Identifier::Pmid(_) => vec![SourceName::PubMed, SourceName::OpenAlex],
        Identifier::Isbn(_) => vec![SourceName::OpenAlex],
    }
}

/// Ask the available sources for `identifier`, in [`priority`] order,
/// and return the first record anybody supplies.
///
/// A source is consulted only when it appears in `sources` and its
/// [`Source::supports`] accepts the identifier; sources not named by
/// [`priority`] are never consulted, whatever `sources` contains. Any
/// failure — not found, unavailable, rate limited, malformed — moves
/// on to the next source, and every failure is recorded in
/// [`Unresolved::attempts`].
///
/// Deterministic: the same sources and identifier always produce the
/// same result and the same attempt list.
pub fn resolve(sources: &[&dyn Source], identifier: &Identifier) -> Result<Resolved, Unresolved> {
    let mut attempts = Vec::new();

    for name in priority(identifier) {
        let consulted = sources
            .iter()
            .filter(|source| source.name() == name && source.supports(identifier));
        for source in consulted {
            match source.fetch(identifier) {
                Ok(record) => {
                    return Ok(Resolved {
                        record,
                        source: name,
                    });
                }
                Err(error) => attempts.push((name, error)),
            }
        }
    }

    Err(Unresolved { attempts })
}
