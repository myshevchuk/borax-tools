//! A content-index entry's references to the libraries holding its
//! bytes: reaching each one, settling what several say, and keeping one
//! in the index.
//!
//! A reference names a library identity, the root it was last known at,
//! the artifact holding the bytes there, and the item that artifact
//! answers with. Reaching one reads that root and writes nothing.
//! Keeping one writes its own file, and repairs references of the same
//! library and artifact whose roots no longer hold that library.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use borax_core::content::ContentHash;
use borax_core::identifier::Identifier;
use borax_core::index::{Fact, LibraryReference};
use borax_core::library::{Item, LibraryId};
use borax_sources::cache::CacheWrite;
use borax_sources::store::{ContentIndex, IndexStore, reference_key};

use crate::event::{AcceptedItemCause, ReferenceStatusStep, ReferenceStep, UnavailableCause};
use crate::library::{Identity, ItemStore, identity};
use crate::pipeline::preferred_identifier;

/// Why a reference's root cannot answer for its library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// Nothing is at the root, a dangling link included.
    Missing,
    /// The root holds no library, or a library with another identity:
    /// `found`, when it holds one. A root that is not a directory holds
    /// none.
    OtherLibrary { found: Option<LibraryId> },
    /// `path` could not be read: the root itself, or its identity file.
    /// `message` is the reader's.
    Unreadable { path: PathBuf, message: String },
}

/// What reaching one reference found.
// The reachable variant carries the item by value, as the answer it
// is; a reach is made once per reference and settled at once, so a box
// would buy an allocation per answer to shrink a value nothing keeps.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Reach {
    /// The root holds the reference's library, and exactly one of its
    /// item files carries the reference's item: this one.
    Reachable(Item),
    /// The root does not hold the reference's library.
    Unavailable(Unavailable),
    /// The root holds the library, and no one item file carries the
    /// reference's item, for the cause given.
    ItemUnavailable(AcceptedItemCause),
}

/// What became of one reference when an entry's references were
/// settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceStatus {
    /// It was reachable and gave the answer.
    Answered,
    /// It was reachable and agreed with the one that answered.
    Agreed,
    /// It was reachable and the reachable references disagreed, so none
    /// answered. `identifier` is its item's preferred identifier, `None`
    /// when the item carries none.
    Disagreeing { identifier: Option<Identifier> },
    /// Its root does not hold its library.
    Unavailable(Unavailable),
    /// Its root holds its library, and its item could not be had.
    ItemUnavailable(AcceptedItemCause),
}

/// One reference of an entry, with what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceRead {
    pub reference: LibraryReference,
    pub status: ReferenceStatus,
}

impl ReferenceRead {
    /// This reference as the stream carries it: its identities as
    /// canonical UUID text, its root, and its status, with an
    /// identifier as it displays.
    pub fn step(&self) -> ReferenceStep {
        let reference = &self.reference;
        ReferenceStep {
            library: reference.library.to_string(),
            root: reference.root.clone(),
            artifact: reference.artifact.to_string(),
            item: reference.item.to_string(),
            status: match &self.status {
                ReferenceStatus::Answered => ReferenceStatusStep::Answered,
                ReferenceStatus::Agreed => ReferenceStatusStep::Agreed,
                ReferenceStatus::Disagreeing { identifier } => ReferenceStatusStep::Disagreeing {
                    identifier: identifier.as_ref().map(Identifier::to_string),
                },
                ReferenceStatus::Unavailable(cause) => ReferenceStatusStep::Unavailable {
                    cause: match cause {
                        Unavailable::Missing => UnavailableCause::Missing,
                        Unavailable::OtherLibrary { found } => UnavailableCause::OtherLibrary {
                            found: found.as_ref().map(LibraryId::to_string),
                        },
                        Unavailable::Unreadable { path, message } => UnavailableCause::Unreadable {
                            path: path.clone(),
                            message: message.clone(),
                        },
                    },
                },
                ReferenceStatus::ItemUnavailable(cause) => ReferenceStatusStep::ItemUnavailable {
                    cause: cause.clone(),
                },
            },
        }
    }
}

/// Whether any of `reads` answered: the reference
/// [`ReferenceStatus::Answered`], when one did.
pub fn answered(reads: &[ReferenceRead]) -> Option<&ReferenceRead> {
    reads
        .iter()
        .find(|read| read.status == ReferenceStatus::Answered)
}

/// The roots one run has read, so that each is read at most once.
///
/// A root's metadata and identity are read the first time a reference
/// names it, and its item store the first time a reference to it names
/// the identity it holds. Every later question about that root answers
/// from those reads, whatever has changed on disk since. Reading writes
/// nothing. Shareable across threads; two threads asking about one root
/// at once read it once.
#[derive(Debug, Default)]
pub struct Libraries {
    roots: Mutex<BTreeMap<PathBuf, Arc<Root>>>,
}

/// What one root was found to hold, each part read once, when first
/// needed.
#[derive(Debug, Default)]
struct Root {
    holds: OnceLock<Holds>,
    items: OnceLock<ItemStore>,
}

/// What a root's metadata and identity said.
#[derive(Debug, Clone)]
enum Holds {
    /// Nothing is at the root.
    Missing,
    /// The root's metadata could not be read.
    Unreadable { message: String },
    /// The root is not a directory.
    NotDirectory,
    /// The root is a directory, and its identity is this.
    Identity(Identity),
}

impl Libraries {
    /// A run's roots, none read yet.
    pub fn new() -> Libraries {
        Libraries::default()
    }

    /// Whether `reference`'s root holds its library and its item, as
    /// that root was first read in this run.
    ///
    /// Decided in this order, the first that holds:
    ///
    /// - nothing is at the root, a dangling link included:
    ///   [`Unavailable::Missing`];
    /// - the root's metadata cannot be read: [`Unavailable::Unreadable`]
    ///   naming the root;
    /// - the root is not a directory, or holds no identity:
    ///   [`Unavailable::OtherLibrary`] with no `found`;
    /// - its identity file cannot be read: [`Unavailable::Unreadable`]
    ///   naming that file;
    /// - it holds another identity: [`Unavailable::OtherLibrary`]
    ///   naming it;
    /// - no one item file carries the reference's item:
    ///   [`Reach::ItemUnavailable`], attributed as
    ///   [`ItemStore::answer`] attributes it;
    /// - otherwise [`Reach::Reachable`] with that item.
    ///
    /// The item is the one the reference stores; the artifact record it
    /// names is not read. The library's marker is not consulted.
    pub fn reach(&self, reference: &LibraryReference) -> Reach {
        let root = self.root(&reference.root);
        let unavailable = |cause: Unavailable| Reach::Unavailable(cause);
        match root.holds(&reference.root) {
            Holds::Missing => unavailable(Unavailable::Missing),
            Holds::Unreadable { message } => unavailable(Unavailable::Unreadable {
                path: reference.root.clone(),
                message: message.clone(),
            }),
            Holds::NotDirectory | Holds::Identity(Identity::Absent) => {
                unavailable(Unavailable::OtherLibrary { found: None })
            }
            Holds::Identity(Identity::Faulty(fault)) => unavailable(Unavailable::Unreadable {
                path: fault.path.clone(),
                message: fault.message.clone(),
            }),
            Holds::Identity(Identity::Present(found)) if found != &reference.library => {
                unavailable(Unavailable::OtherLibrary {
                    found: Some(found.clone()),
                })
            }
            Holds::Identity(Identity::Present(_)) => {
                let items = root.items.get_or_init(|| ItemStore::read(&reference.root));
                match items.answer(&reference.root, &reference.item) {
                    Ok(item) => Reach::Reachable(item.clone()),
                    Err(cause) => Reach::ItemUnavailable(cause),
                }
            }
        }
    }

    /// Whether a reference to `library` at `root` is stale, as that root
    /// was first read in this run: nothing is at it, it is not a
    /// directory, it holds no identity, or it holds another one.
    ///
    /// A root holding `library` is not stale, and neither is one whose
    /// metadata or identity file cannot be read: a library that cannot
    /// be read is not known to be gone.
    pub fn is_stale(&self, root: &Path, library: &LibraryId) -> bool {
        self.root(root).holds(root).is_stale_for(library)
    }

    /// The record of `root`, made empty the first time it is asked for.
    fn root(&self, root: &Path) -> Arc<Root> {
        // A lock poisoned by a panicking reader still guards a map whose
        // every value is whole, since each is filled through its own
        // `OnceLock`.
        let mut roots = self
            .roots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(roots.entry(root.to_path_buf()).or_default())
    }
}

impl Root {
    /// What the root at `path` holds, read the first time it is asked.
    fn holds(&self, path: &Path) -> &Holds {
        self.holds.get_or_init(|| Holds::read(path))
    }
}

impl Holds {
    /// What the root at `path` holds, read from disk now.
    fn read(path: &Path) -> Holds {
        match fs::metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Holds::Missing,
            Err(error) => Holds::Unreadable {
                message: error.to_string(),
            },
            Ok(metadata) if !metadata.is_dir() => Holds::NotDirectory,
            Ok(_) => Holds::Identity(identity(path)),
        }
    }

    /// Whether a root holding this no longer holds `library`: it is
    /// missing, is not a directory, holds no identity, or holds another
    /// one. A root that cannot be read is not stale.
    fn is_stale_for(&self, library: &LibraryId) -> bool {
        match self {
            Holds::Missing | Holds::NotDirectory | Holds::Identity(Identity::Absent) => true,
            Holds::Identity(Identity::Present(found)) => found != library,
            Holds::Unreadable { .. } | Holds::Identity(Identity::Faulty(_)) => false,
        }
    }
}

/// The answer several reached references give, and every reference's
/// status, in the order given.
///
/// The reachable references agree when every one's item carries the
/// same [`preferred_identifier`], or when none carries one and all are
/// one item of one library. Agreeing, the first reachable reference is
/// [`ReferenceStatus::Answered`] and gives the answer, and the others
/// are [`ReferenceStatus::Agreed`]. Disagreeing, each is
/// [`ReferenceStatus::Disagreeing`] with its item's identifier, and
/// none answers. An unreachable reference keeps its cause either way.
/// With nothing reachable, nothing answers.
pub fn settle(
    reached: Vec<(LibraryReference, Reach)>,
) -> (Option<(LibraryReference, Item)>, Vec<ReferenceRead>) {
    let reachable: Vec<(&LibraryReference, &Item, Option<Identifier>)> = reached
        .iter()
        .filter_map(|(reference, reach)| match reach {
            Reach::Reachable(item) => Some((reference, item, preferred_identifier(&item.record))),
            Reach::Unavailable(_) | Reach::ItemUnavailable(_) => None,
        })
        .collect();
    let agree = match reachable.split_first() {
        None => false,
        Some(((first, item, identifier), rest)) => {
            rest.iter()
                .all(|(other, held, theirs)| match (identifier, theirs) {
                    (Some(ours), Some(theirs)) => ours == theirs,
                    (None, None) => other.library == first.library && held.id == item.id,
                    _ => false,
                })
        }
    };
    let answer = reachable
        .first()
        .filter(|_| agree)
        .map(|(reference, item, _)| ((*reference).clone(), (*item).clone()));

    let mut answered = false;
    let reads = reached
        .into_iter()
        .map(|(reference, reach)| {
            let status = match reach {
                Reach::Unavailable(cause) => ReferenceStatus::Unavailable(cause),
                Reach::ItemUnavailable(cause) => ReferenceStatus::ItemUnavailable(cause),
                Reach::Reachable(_) if agree && !answered => {
                    answered = true;
                    ReferenceStatus::Answered
                }
                Reach::Reachable(_) if agree => ReferenceStatus::Agreed,
                Reach::Reachable(item) => ReferenceStatus::Disagreeing {
                    identifier: preferred_identifier(&item.record),
                },
            };
            ReferenceRead { reference, status }
        })
        .collect();
    (answer, reads)
}

/// Read every reference of `hash`'s entry, in entry order, reach each
/// through `libraries`, and [`settle`] them. Writes nothing.
pub fn read<S: IndexStore>(
    index: &ContentIndex<S>,
    hash: &ContentHash,
    libraries: &Libraries,
) -> (Option<(LibraryReference, Item)>, Vec<ReferenceRead>) {
    settle(
        index
            .references(hash)
            .into_iter()
            .map(|reference| {
                let reach = libraries.reach(&reference);
                (reference, reach)
            })
            .collect(),
    )
}

/// What keeping one reference wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The reference written.
    pub reference: LibraryReference,
    /// The roots of the stale references removed after it was written,
    /// in entry order. Empty when its key already had a file, and when
    /// the write failed.
    pub repaired_from: Vec<PathBuf>,
    /// The write's result: [`CacheWrite::Failed`] when the reference
    /// could not be written, or when a stale reference could not be
    /// removed, naming each it could not.
    pub write: CacheWrite,
}

/// Keep `reference` in `hash`'s entry, repairing stale references to
/// the same library and artifact, and say what was written; `None` when
/// the file of its key already holds it with the same item, and nothing
/// is written.
///
/// - **The key's own file.** One holding another item, or that cannot be
///   read or parsed, is replaced whole with `reference`, and no other
///   reference is touched.
/// - **No file.** The entry's readable references naming the same
///   library and artifact at another root that `libraries` finds stale
///   ([`Libraries::is_stale`]) are found first. `reference` is written,
///   and only then is each of them removed, if its file still holds
///   that reference and its root, read again from disk, is still stale;
///   one another run rewrote, or whose root holds the library again, is
///   left and counts as neither removed nor failed. A write that fails
///   removes nothing. A removal that fails leaves that file, and the result is
///   [`CacheWrite::Failed`] naming it; a reference already gone counts
///   as removed.
///
/// No other reference is ever removed: not one whose key had a file,
/// not an unreadable one, and not one of another artifact or library.
/// No evidence fact is read or written.
pub fn keep<S: IndexStore>(
    index: &ContentIndex<S>,
    hash: &ContentHash,
    reference: &LibraryReference,
    libraries: &Libraries,
) -> Option<Kept> {
    let written = |write: CacheWrite, repaired_from: Vec<PathBuf>| Kept {
        reference: reference.clone(),
        repaired_from,
        write,
    };
    match index.reference(hash, reference) {
        Fact::Held(held) if held.item == reference.item => return None,
        Fact::Held(_) | Fact::Unreadable(_) => {
            return Some(written(index.put_reference(hash, reference), Vec::new()));
        }
        Fact::Absent => {}
    }

    let stale: Vec<LibraryReference> = index
        .references(hash)
        .into_iter()
        .filter(|other| {
            other.library == reference.library
                && other.artifact == reference.artifact
                && other.root != reference.root
                && libraries.is_stale(&other.root, &other.library)
        })
        .collect();
    let write = index.put_reference(hash, reference);
    if matches!(write, CacheWrite::Failed { .. }) {
        return Some(written(write, Vec::new()));
    }

    let mut repaired_from = Vec::new();
    let mut failures = Vec::new();
    // Another run may have acted since the entry was listed: it may
    // have rewritten a stale reference, or its root may hold the
    // library again. Each is checked afresh, from disk and not from the
    // run's first read, just before it would be removed, and one that
    // is no longer the stale reference listed is left.
    let still_stale = |old: &LibraryReference| {
        matches!(index.reference(hash, old), Fact::Held(held) if held == *old)
            && Holds::read(&old.root).is_stale_for(&old.library)
    };
    for old in stale.into_iter().filter(still_stale) {
        match index.remove_reference(hash, &old) {
            CacheWrite::Written => repaired_from.push(old.root),
            CacheWrite::Failed { message } => failures.push(format!(
                "cannot remove the stale reference to {} ({}): {message}",
                old.root.display(),
                reference_key(hash, &old.name().unwrap_or_default()),
            )),
        }
    }
    let write = match failures.is_empty() {
        true => CacheWrite::Written,
        false => CacheWrite::Failed {
            message: failures.join("; "),
        },
    };
    Some(written(write, repaired_from))
}
