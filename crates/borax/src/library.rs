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

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use borax_core::content::ContentHash;
use borax_core::identifier::Identifier;
use borax_core::library::{
    ArtifactId, ArtifactRecord, Item, ItemId, is_library_relative, is_well_formed_hash, name_uuid,
};
use borax_core::record::Record;
use uuid::Uuid;

use crate::config::OVERRIDE_FILE;
use crate::event::{Event, Finding};
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

/// Why the library rooted at `root` does not own `directory`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Excluded {
    /// The library's own state directory, [`STATE_DIR`].
    State,
    /// The library's own item store, [`ITEM_STORE`].
    Items,
    /// A directory holding an [`OVERRIDE_FILE`] of its own, which makes
    /// it a library in its own right.
    Nested,
}

/// Why the library rooted at `root` does not own `directory`, or `None`
/// when it does.
///
/// The three subtrees it does not own are its own state directory, its
/// item store, and a nested library. The root always owns itself,
/// whether or not it holds the marker that made it a root.
///
/// This answers about `directory` alone and says nothing about what
/// lies above it; [`excludes`] is the same rule applied to a whole
/// path. The reason is distinguished rather than collapsed because a
/// walk reports the nested libraries it stopped at and must not report
/// this library's own directories among them.
fn ownership(root: &Path, directory: &Path) -> Option<Excluded> {
    if same_name(root, directory) {
        return None;
    }
    if same_name(directory, &root.join(STATE_DIR)) {
        return Some(Excluded::State);
    }
    if same_name(directory, &root.join(ITEM_STORE)) {
        return Some(Excluded::Items);
    }
    match directory.join(OVERRIDE_FILE).is_file() {
        true => Some(Excluded::Nested),
        false => None,
    }
}

/// Whether the library rooted at `root` owns `directory` itself:
/// [`ownership`] as the predicate a walk asks.
fn owns(root: &Path, directory: &Path) -> bool {
    ownership(root, directory).is_none()
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
    walk(root).0
}

/// One walk of the library rooted at `root`: its artifacts sorted by
/// path, and the nested libraries the walk stopped at, library-relative
/// and in path order.
///
/// The two answers come of one descent under one rule ([`owns`]), so
/// the files counted and the subtrees left out cannot disagree about
/// where the library stops.
fn walk(root: &Path) -> (Vec<PathBuf>, Vec<String>) {
    let nested = RefCell::new(Vec::new());
    let mut found = Vec::new();
    documents(
        root,
        &|directory| match ownership(root, directory) {
            None => true,
            Some(excluded) => {
                if excluded == Excluded::Nested {
                    if let Some(relative) = library_relative(root, directory) {
                        nested.borrow_mut().push(relative);
                    }
                }
                false
            }
        },
        &mut found,
    );

    found.sort();
    let mut nested = nested.into_inner();
    nested.sort();
    (found, nested)
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
    /// Every item read, each beside the file it was read from, in path
    /// order. Validation is about files, so the path travels with the
    /// record rather than being recoverable from it — an item file
    /// renamed by hand is still the item it was, and saying so is a
    /// finding about that name.
    entries: Vec<(PathBuf, Item)>,
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
                    store.entries.push((path, parsed.item));
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
        self.iter().find(|item| &item.id == id)
    }

    /// The first item whose record carries `identifier`, or `None` when
    /// none does.
    ///
    /// Only the field of `identifier`'s own kind is compared, so a DOI
    /// is looked for among DOIs alone. Items are searched in the order
    /// they were read, which is the store's path order.
    pub fn by_identifier(&self, identifier: &Identifier) -> Option<&Item> {
        self.iter()
            .find(|item| identifier_of(&item.record, identifier).as_ref() == Some(identifier))
    }

    /// How many items were read.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no item was read. A store with faults and no items is
    /// empty: a file that cost its record contributes none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every item read, in the order they were read.
    pub fn iter(&self) -> impl Iterator<Item = &Item> {
        self.entries.iter().map(|(_, item)| item)
    }

    /// Every item read, each beside the file it came from, in the order
    /// they were read.
    fn files(&self) -> impl Iterator<Item = (&Path, &Item)> {
        self.entries
            .iter()
            .map(|(path, item)| (path.as_path(), item))
    }
}

/// A library's artifact records, as read.
///
/// Every `.toml` file under `.borax/artifacts` that parses as a record,
/// held by value on the same terms as [`ItemStore`].
#[derive(Debug, Clone, Default)]
pub struct ArtifactStore {
    /// Every record read, each beside the file it was read from, in
    /// path order, on the same terms as [`ItemStore`]'s.
    entries: Vec<(PathBuf, ArtifactRecord)>,
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
            let parsed = text.and_then(|text| {
                ArtifactRecord::from_toml(&text).map_err(|error| StoreFault {
                    path: path.clone(),
                    message: error.to_string(),
                })
            });
            match parsed {
                Ok(record) => store.entries.push((path, record)),
                Err(fault) => store.faults.push(fault),
            }
        }

        store
    }

    /// The record of identity `id`, or `None` when the store holds
    /// none.
    pub fn by_id(&self, id: &ArtifactId) -> Option<&ArtifactRecord> {
        self.iter().find(|record| &record.id == id)
    }

    /// Every record whose history holds `hash`, in read order.
    ///
    /// The whole history is searched, not only the newest entry, so an
    /// artifact is found by bytes it used to have as well as by the
    /// bytes it has. Several records can share a hash: two files of
    /// identical content are two artifacts.
    pub fn by_hash(&self, hash: &ContentHash) -> Vec<&ArtifactRecord> {
        self.iter().filter(|record| record.holds(hash)).collect()
    }

    /// Every record linked to the item `item`, in read order. An item
    /// may have several artifacts.
    pub fn by_item(&self, item: &ItemId) -> Vec<&ArtifactRecord> {
        self.iter()
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
        self.iter().find(|record| record.path == relative)
    }

    /// How many records were read.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no record was read.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every record read, in the order they were read.
    pub fn iter(&self) -> impl Iterator<Item = &ArtifactRecord> {
        self.entries.iter().map(|(_, record)| record)
    }

    /// Every record read, each beside the file it came from, in the
    /// order they were read.
    fn files(&self) -> impl Iterator<Item = (&Path, &ArtifactRecord)> {
        self.entries
            .iter()
            .map(|(path, record)| (path.as_path(), record))
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

/// What a library holds, as one walk of its tree and one read of its
/// two stores found it.
///
/// The counts `borax status` reports, with the paths behind two of them
/// kept rather than only their totals: a caller asked for more — the
/// identifiable count — needs the artifacts themselves, and a walk is
/// the one part of the survey worth not doing twice.
///
/// Nothing here opens a document. The cost of a survey is the directory
/// walk and the store read, which is what lets a library borax has
/// never seen be reported on as cheaply as one it wrote itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Survey {
    /// The root of what was surveyed. The library root when a marker
    /// or a configured `library-root` established one, and the
    /// directory the run was given when neither did — a directory
    /// nobody marked is reported on as given.
    pub root: PathBuf,
    /// Every artifact in the tree, sorted by path.
    pub artifacts: Vec<PathBuf>,
    /// The artifacts no record names, in the order [`Survey::artifacts`]
    /// holds them.
    pub orphans: Vec<PathBuf>,
    /// The nested libraries the walk stopped at, library-relative and
    /// in path order.
    ///
    /// A marker below the root takes its whole subtree out of every
    /// count above it, so a reader who cannot see what was excluded
    /// cannot tell a small library from a subdivided one.
    pub nested: Vec<String>,
    /// How many items the item store holds.
    pub items: usize,
    /// How many records the artifact store holds.
    pub records: usize,
}

/// Survey the library rooted at `root`.
///
/// Never fails and opens no document: a store file that cannot be read
/// costs its own record and no other ([`ItemStore::read`]), and a
/// directory that cannot be listed contributes nothing ([`artifacts`]).
/// A library with no store directories at all is a library of orphans,
/// which is what a directory borax has never seen is.
pub fn survey(root: &Path) -> Survey {
    surveyed(root, &contents(root))
}

/// A library as read: one walk of its tree and one read of each of its
/// stores, which is everything [`survey`] and [`validate`] are made of.
struct Contents {
    artifacts: Vec<PathBuf>,
    nested: Vec<String>,
    items: ItemStore,
    records: ArtifactStore,
}

/// Read the library rooted at `root`. Never fails, and opens no
/// document.
fn contents(root: &Path) -> Contents {
    let (artifacts, nested) = walk(root);
    Contents {
        artifacts,
        nested,
        items: ItemStore::read(root),
        records: ArtifactStore::read(root),
    }
}

/// What `contents`, read from the library rooted at `root`, amounts to
/// as a survey.
fn surveyed(root: &Path, contents: &Contents) -> Survey {
    Survey {
        root: root.to_path_buf(),
        orphans: orphans(root, &contents.artifacts, &contents.records),
        artifacts: contents.artifacts.clone(),
        nested: contents.nested.clone(),
        items: contents.items.len(),
        records: contents.records.len(),
    }
}

/// Whether the name of `path` carries an identity other than `id` —
/// including a name carrying no canonical UUID at all, which carries
/// none of them.
fn name_disagrees(path: &Path, id: Uuid) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name_uuid(&name) != Some(id)
}

/// Every finding about the item store's own files, in path order.
///
/// A file that cost its record is reported by the fault the store
/// already holds; a file that yielded one is judged on its name and on
/// the identity inside it.
fn item_findings(items: &ItemStore) -> Vec<(PathBuf, Finding)> {
    let mut found: Vec<(PathBuf, Finding)> = items
        .faults
        .iter()
        .map(|fault| {
            (
                fault.path.clone(),
                Finding::Unreadable {
                    message: fault.message.clone(),
                },
            )
        })
        .collect();

    let mut seen: Vec<(&ItemId, &Path)> = Vec::new();
    for (path, item) in items.files() {
        let first = seen
            .iter()
            .find(|(id, _)| *id == &item.id)
            .map(|(_, first)| first.to_path_buf());
        match first {
            Some(first) => found.push((
                path.to_path_buf(),
                Finding::DuplicateIdentity {
                    id: item.id.to_string(),
                    other: first,
                },
            )),
            None => seen.push((&item.id, path)),
        }
        if name_disagrees(path, item.id.uuid()) {
            found.push((
                path.to_path_buf(),
                Finding::NameDisagrees {
                    id: item.id.to_string(),
                },
            ));
        }
    }

    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

/// Every finding about the artifact store's own files, in path order.
///
/// `items` decides which item links dangle: a record naming an item no
/// item file carries is a link to nothing.
fn record_findings(records: &ArtifactStore, items: &ItemStore) -> Vec<(PathBuf, Finding)> {
    let mut found: Vec<(PathBuf, Finding)> = records
        .faults
        .iter()
        .map(|fault| {
            (
                fault.path.clone(),
                Finding::Unreadable {
                    message: fault.message.clone(),
                },
            )
        })
        .collect();

    let mut seen: Vec<(&ArtifactId, &Path)> = Vec::new();
    for (path, record) in records.files() {
        let mut about = |finding| found.push((path.to_path_buf(), finding));

        if let Some(item) = &record.item {
            if items.by_id(item).is_none() {
                about(Finding::DanglingItem {
                    item: item.to_string(),
                });
            }
        }
        let first = seen
            .iter()
            .find(|(id, _)| *id == &record.id)
            .map(|(_, first)| first.to_path_buf());
        match first {
            Some(first) => about(Finding::DuplicateIdentity {
                id: record.id.to_string(),
                other: first,
            }),
            None => seen.push((&record.id, path)),
        }
        if name_disagrees(path, record.id.uuid()) {
            about(Finding::NameDisagrees {
                id: record.id.to_string(),
            });
        }
        if !is_library_relative(&record.path) {
            about(Finding::PathNotRelative {
                path: record.path.clone(),
            });
        }
        if record.history.is_empty() {
            about(Finding::EmptyHistory);
        }
        for entry in &record.history {
            if !is_well_formed_hash(&entry.hash) {
                about(Finding::MalformedHash {
                    hash: entry.hash.to_string(),
                });
            }
            if entry.run.as_str().is_empty() {
                about(Finding::HistoryEntryWithoutRun {
                    hash: entry.hash.to_string(),
                });
            }
        }
    }

    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

/// The event reporting `survey`, carrying `identifiable` as the count
/// of artifacts an identifier could be extracted from.
///
/// `identifiable` is `None` when the run was not asked for it: the
/// survey itself never opens a document, so the count comes from the
/// caller that did.
pub fn status_event(survey: &Survey, identifiable: Option<usize>) -> Event {
    Event::LibraryStatus {
        root: survey.root.clone(),
        artifacts: survey.artifacts.len(),
        items: survey.items,
        records: survey.records,
        orphans: survey.orphans.len(),
        nested: survey.nested.clone(),
        identifiable,
    }
}

/// What `borax validate` found about a library.
///
/// The findings are what is wrong with the library's own records. The
/// three counts are not findings and never become them: an orphan is
/// work to do, an artifact borax cannot find is history the library
/// deliberately keeps, and an item nothing links to is an ordinary item
/// for a work with no file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validation {
    /// The library the findings are about, as [`Survey::root`].
    pub root: PathBuf,
    /// Every finding, each with the file it is about, in the order the
    /// stores were read: the item store first, then the artifact
    /// records, each in path order.
    pub findings: Vec<(PathBuf, Finding)>,
    /// Artifacts no record names.
    pub orphans: usize,
    /// Records whose last-known path holds no file.
    pub missing: usize,
    /// Items no artifact record links to.
    pub unlinked: usize,
}

/// Validate the library rooted at `root`.
///
/// Reads the tree and both stores and reports what it found. Repairs
/// nothing, refuses nothing, writes nothing and holds no lock: a
/// library has no single consistent state at any moment, its writers
/// being peers, so a file observed half-written is a finding about that
/// file and about nothing else.
pub fn validate(root: &Path) -> Validation {
    let contents = contents(root);
    let survey = surveyed(root, &contents);

    let mut findings = item_findings(&contents.items);
    findings.extend(record_findings(&contents.records, &contents.items));

    Validation {
        root: survey.root,
        findings,
        orphans: survey.orphans.len(),
        missing: missing(root, &contents.records, &|path| path.is_file()).len(),
        unlinked: contents
            .items
            .iter()
            .filter(|item| contents.records.by_item(&item.id).is_empty())
            .count(),
    }
}

/// The events reporting `validation`: one per finding in the order they
/// were found, then the totals.
///
/// The totals come last so that a reader of the stream has the findings
/// before the count of them, as every other run reports its files
/// before its summary.
pub fn validation_events(validation: &Validation) -> Vec<Event> {
    let mut events: Vec<Event> = validation
        .findings
        .iter()
        .map(|(path, finding)| Event::LibraryFinding {
            path: path.clone(),
            finding: finding.clone(),
        })
        .collect();

    events.push(Event::LibraryValidated {
        root: validation.root.clone(),
        findings: validation.findings.len(),
        orphans: validation.orphans,
        missing: validation.missing,
        unlinked: validation.unlinked,
    });
    events
}
