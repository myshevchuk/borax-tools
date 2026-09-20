//! A library on disk: where its boundary runs, which files are its
//! artifacts, and what its two stores say.
//!
//! A library is one directory tree. The directory holding the nearest
//! `.borax.toml` is its root; `.borax/` under that root holds the state
//! borax writes, and `items/` holds the record of every work the
//! library knows. Everything else in the tree is the operator's, and
//! the artifacts are the PDFs among it.
//!
//! [`borax_core::library`] holds the values — the two identities, the
//! two records, the TOML each is written as — and touches no file. This
//! module is the adapter around them: the boundary predicates, the
//! artifact walk, and the two stores, each read straight from the files
//! themselves with no index in between. The library's text is
//! authoritative, so nothing here caches it beyond the call that read
//! it.

use std::fs;
use std::path::{Path, PathBuf};

use borax_core::content::ContentHash;
use borax_core::identifier::Identifier;
use borax_core::library::{ArtifactId, ArtifactRecord, Item, ItemId};
use borax_core::record::Record;

use crate::config::OVERRIDE_FILE;
use crate::paths::{lexical, same_name};
use crate::run::documents;

pub use crate::config::library_root;

/// The directory a library keeps its state in, directly under the
/// library root.
pub const STATE_DIR: &str = ".borax";

/// The directory holding the library's items, directly under the
/// library root.
pub const ITEM_STORE: &str = "items";

/// The directory holding the artifact records, under [`STATE_DIR`].
pub const ARTIFACT_STORE: &str = "artifacts";

/// The extension a library file is written with.
const RECORD_EXTENSION: &str = "toml";

/// Whether `path` lies at or below `root`.
///
/// Both sides are normalised lexically first — `.` dropped, `..`
/// cancelled, a relative path resolved against the working directory —
/// and components are then matched the way the platform matches file
/// names. Containment is by component, so a sibling directory whose
/// name merely begins with the root's is outside it.
///
/// Symlinks are not resolved. A link inside the tree is inside it by
/// its own path whatever it points at, and its target keeps its own
/// path and stays wherever that lies. `false` when either side fails to
/// normalise, which only a relative path with no working directory to
/// resolve it against can do.
pub fn contains(root: &Path, path: &Path) -> bool {
    let (Some(root), Some(path)) = (lexical(root), lexical(path)) else {
        return false;
    };

    let mut root = root.components();
    let mut path = path.components();
    loop {
        match (root.next(), path.next()) {
            (None, _) => return true,
            (Some(_), None) => return false,
            (Some(expected), Some(found)) => {
                if !same_name(
                    Path::new(expected.as_os_str()),
                    Path::new(found.as_os_str()),
                ) {
                    return false;
                }
            }
        }
    }
}

/// `path` as a library-relative path: relative to `root` and
/// `/`-separated whatever the platform writes, which is the form an
/// artifact record stores.
///
/// `None` when `path` is not spelled as a descendant of `root`. The
/// comparison is on the paths as given, so both sides have to be
/// spelled the same way — which they are when `path` came from
/// [`artifacts`].
pub fn library_relative(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let segments: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(segments.join("/"))
}

/// The full path of `relative`, which is `/`-separated and relative to
/// `root` as an artifact record's path is.
///
/// The separator is the record's own rather than the platform's, so a
/// record written on one machine names the same file on another.
pub fn relative_to(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, segment| path.join(segment))
}

/// Whether the library rooted at `root` owns `directory` itself.
///
/// The three subtrees it does not own are its own state directory, its
/// item store, and any directory below the root holding an
/// [`OVERRIDE_FILE`] of its own — that directory is a library in its
/// own right and its files are its own. The root always owns itself,
/// whether or not it holds the marker that made it a root.
///
/// This answers about `directory` alone and says nothing about what
/// lies above it; [`excludes`] is the same rule applied to a whole
/// path.
fn owns(root: &Path, directory: &Path) -> bool {
    if same_name(root, directory) {
        return true;
    }
    !same_name(directory, &root.join(STATE_DIR))
        && !same_name(directory, &root.join(ITEM_STORE))
        && !directory.join(OVERRIDE_FILE).is_file()
}

/// Whether the library rooted at `root` excludes `path`.
///
/// True for a path outside the tree, and for one inside it lying under
/// a directory the root does not own: the state directory, the item
/// store, or a nested library. This is the one rule the artifact walk,
/// the orphan count, an applying run's admissions and reconciliation
/// all ask, so that the four agree about where the library stops.
///
/// `path` is compared against `root` as spelled, in the form
/// [`artifacts`] produces.
pub fn excludes(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };

    let mut walked = root.to_path_buf();
    for component in relative.components() {
        walked.push(component);
        if !owns(root, &walked) {
            return true;
        }
    }
    false
}

/// Every artifact of the library rooted at `root`, sorted by path.
///
/// An artifact is a file below the root, at any depth, whose extension
/// is [`PDF_EXTENSION`](crate::run::PDF_EXTENSION)
/// ignoring case. Nothing the root does not own
/// contributes — the state directory, the item store, and a nested
/// library are walked past whole, so a PDF placed in any of them is not
/// this library's artifact. A sidecar is excluded by its own extension,
/// and a symlink by being neither a file nor a directory to a walk that
/// does not follow links.
///
/// A directory that cannot be read contributes nothing rather than
/// failing the walk, so one unreadable subtree costs its own files and
/// no others.
pub fn artifacts(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    documents(root, &|directory| owns(root, directory), &mut found);
    found.sort();
    found
}

/// A file of a library's store that could not be read as the record it
/// claims to be.
///
/// One fault is one file: the store answers from everything else it
/// read, so an unreadable or unparsable file costs its own record and
/// no other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreFault {
    /// The file the fault is about.
    pub path: PathBuf,
    /// Why it could not be read, as the reader or the parser put it.
    pub message: String,
}

/// Every file of `directory` with the library's record extension, in
/// path order, each as its text or the fault reading it produced.
///
/// A directory that is not there yields nothing and no fault: a library
/// that has recorded nothing yet has no store to read. A directory
/// entry that cannot be listed is passed over for the same reason a
/// file that cannot be read is a fault about itself alone.
fn store_files(directory: &Path) -> Vec<(PathBuf, Result<String, StoreFault>)> {
    let Ok(listing) = fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = listing
        .flatten()
        .filter(|entry| entry.metadata().is_ok_and(|metadata| metadata.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case(RECORD_EXTENSION))
        })
        .collect();
    paths.sort();

    paths
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).map_err(|error| StoreFault {
                path: path.clone(),
                message: error.to_string(),
            });
            (path, text)
        })
        .collect()
}

/// The identifier of `record` of the same kind as `wanted`, if it
/// carries one.
fn identifier_of(record: &Record, wanted: &Identifier) -> Option<Identifier> {
    match wanted {
        Identifier::Doi(_) => record.doi.clone().map(Identifier::Doi),
        Identifier::Arxiv(_) => record.borax.arxiv.clone().map(Identifier::Arxiv),
        Identifier::Pmid(_) => record.pmid.map(Identifier::Pmid),
        Identifier::Isbn(_) => record.isbn.clone().map(Identifier::Isbn),
    }
}

/// A library's items, as read.
///
/// Every `.toml` file directly under the library's [`ITEM_STORE`] that
/// parses as an item, held by value: the store is a snapshot of the
/// text at the moment it was read, and re-reading is how a caller sees
/// a later edit.
#[derive(Debug, Clone, Default)]
pub struct ItemStore {
    items: Vec<Item>,
    /// The files that cost a record, and the source fields that were
    /// dropped from one that survived, in path order.
    pub faults: Vec<StoreFault>,
}

impl ItemStore {
    /// The item store of the library rooted at `root`.
    ///
    /// Never fails. A store directory that is not there is an empty
    /// store, and a file that is not readable or does not parse as an
    /// item is a [`StoreFault`] naming it, leaving every other file
    /// read. A source field whose stored text does not parse is a fault
    /// too, and the item it was dropped from is still read.
    pub fn read(root: &Path) -> ItemStore {
        let mut store = ItemStore::default();

        for (path, text) in store_files(&root.join(ITEM_STORE)) {
            let text = match text {
                Ok(text) => text,
                Err(fault) => {
                    store.faults.push(fault);
                    continue;
                }
            };

            match Item::from_toml(&text) {
                Ok(parsed) => {
                    store
                        .faults
                        .extend(parsed.faults.iter().map(|fault| StoreFault {
                            path: path.clone(),
                            message: format!("dropped unreadable source field {:?}", fault.key),
                        }));
                    store.items.push(parsed.item);
                }
                Err(error) => store.faults.push(StoreFault {
                    path,
                    message: error.to_string(),
                }),
            }
        }

        store
    }

    /// The item of identity `id`, or `None` when the store holds none.
    ///
    /// The identity is the one inside the file, never the one its name
    /// claims: an item file renamed by hand is still the item it was.
    pub fn by_id(&self, id: &ItemId) -> Option<&Item> {
        self.items.iter().find(|item| &item.id == id)
    }

    /// The first item whose record carries `identifier`, or `None` when
    /// none does.
    ///
    /// Only the field of `identifier`'s own kind is compared, so a DOI
    /// is looked for among DOIs alone. Items are searched in the order
    /// they were read, which is the store's path order.
    pub fn by_identifier(&self, identifier: &Identifier) -> Option<&Item> {
        self.items
            .iter()
            .find(|item| identifier_of(&item.record, identifier).as_ref() == Some(identifier))
    }

    /// How many items were read.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no item was read. A store with faults and no items is
    /// empty: a file that cost its record contributes none.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Every item read, in the order they were read.
    pub fn iter(&self) -> impl Iterator<Item = &Item> {
        self.items.iter()
    }
}

/// A library's artifact records, as read.
///
/// Every `.toml` file under `.borax/artifacts` that parses as a record,
/// held by value on the same terms as [`ItemStore`].
#[derive(Debug, Clone, Default)]
pub struct ArtifactStore {
    records: Vec<ArtifactRecord>,
    /// The files that cost a record, in path order.
    pub faults: Vec<StoreFault>,
}

impl ArtifactStore {
    /// The artifact-record store of the library rooted at `root`.
    ///
    /// Never fails, on the same terms as [`ItemStore::read`]: an absent
    /// store directory is an empty store, and one file that cannot be
    /// read or does not parse is a [`StoreFault`] costing its own
    /// record and no other.
    pub fn read(root: &Path) -> ArtifactStore {
        let mut store = ArtifactStore::default();

        for (path, text) in store_files(&root.join(STATE_DIR).join(ARTIFACT_STORE)) {
            match text.and_then(|text| {
                ArtifactRecord::from_toml(&text).map_err(|error| StoreFault {
                    path: path.clone(),
                    message: error.to_string(),
                })
            }) {
                Ok(record) => store.records.push(record),
                Err(fault) => store.faults.push(fault),
            }
        }

        store
    }

    /// The record of identity `id`, or `None` when the store holds
    /// none.
    pub fn by_id(&self, id: &ArtifactId) -> Option<&ArtifactRecord> {
        self.records.iter().find(|record| &record.id == id)
    }

    /// Every record whose history holds `hash`, in read order.
    ///
    /// The whole history is searched, not only the newest entry, so an
    /// artifact is found by bytes it used to have as well as by the
    /// bytes it has. Several records can share a hash: two files of
    /// identical content are two artifacts.
    pub fn by_hash(&self, hash: &ContentHash) -> Vec<&ArtifactRecord> {
        self.records
            .iter()
            .filter(|record| record.holds(hash))
            .collect()
    }

    /// Every record linked to the item `item`, in read order. An item
    /// may have several artifacts.
    pub fn by_item(&self, item: &ItemId) -> Vec<&ArtifactRecord> {
        self.records
            .iter()
            .filter(|record| record.item.as_ref() == Some(item))
            .collect()
    }

    /// The first record whose last-known path is `relative`, or `None`
    /// when none is.
    ///
    /// `relative` is library-relative and `/`-separated, the form
    /// [`library_relative`] produces. The path is where the artifact
    /// was last seen and not where it must be, so a record answering
    /// here says nothing about whether the file is still there.
    pub fn by_path(&self, relative: &str) -> Option<&ArtifactRecord> {
        self.records.iter().find(|record| record.path == relative)
    }

    /// How many records were read.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether no record was read.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Every record read, in the order they were read.
    pub fn iter(&self) -> impl Iterator<Item = &ArtifactRecord> {
        self.records.iter()
    }
}

/// The artifacts among `artifacts` that no record in `records` names,
/// in the order given.
///
/// `artifacts` is the walk's output for the library rooted at `root`,
/// and an orphan is a file the library holds and has no record of. A
/// record whose last-known path holds no file is not an orphan: nothing
/// was found there to orphan, and the finding about it is
/// [`missing`].
pub fn orphans(root: &Path, artifacts: &[PathBuf], records: &ArtifactStore) -> Vec<PathBuf> {
    artifacts
        .iter()
        .filter(|path| match library_relative(root, path) {
            Some(relative) => records.by_path(&relative).is_none(),
            None => false,
        })
        .cloned()
        .collect()
}

/// The records of `records` whose last-known path holds no file, in
/// read order.
///
/// `exists` answers whether a path is there, and disk is what decides.
/// Each record's path is resolved against `root` the way
/// [`relative_to`] resolves one. This is the opposite finding to
/// [`orphans`]: a record with no artifact rather than an artifact with
/// no record.
pub fn missing<'a>(
    root: &Path,
    records: &'a ArtifactStore,
    exists: &dyn Fn(&Path) -> bool,
) -> Vec<&'a ArtifactRecord> {
    records
        .iter()
        .filter(|record| !exists(&relative_to(root, &record.path)))
        .collect()
}
