//! A library on disk: where its boundary runs, which files are its
//! artifacts, what its two stores say, and which library it is.
//!
//! A library is one directory tree. The directory holding the nearest
//! `.borax.toml` is its root; `.borax/` under that root holds the state
//! borax writes, the library's identity among it, and `items/` holds
//! the record of every work the library knows. Everything else in the
//! tree is the operator's, and the artifacts are the PDFs among it.
//!
//! [`borax_core::library`] holds the values — the item, artifact and
//! library identities, the records and the identity file, the TOML each
//! is written as — and touches no file. This module is the adapter
//! around them: the boundary predicates, the artifact walk, the two
//! stores and the identity file, each read straight from the files
//! themselves with no index in between, and the writer every store
//! write goes through. The library's text is authoritative, so nothing
//! here caches it beyond the call that read it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Components, Path, PathBuf};
use std::time::UNIX_EPOCH;

use borax_core::content::ContentHash;
use borax_core::identifier::{Identifier, supplied};
use borax_core::library::{
    ArtifactId, ArtifactRecord, EvidenceOrigin, HashEntry, IdentificationEvidence, Item, ItemId,
    LibraryId, LibraryIdentity, RunId, is_library_relative, is_well_formed_hash, name_uuid,
};
use borax_core::record::Record;
use borax_core::template::slug;
use borax_sources::source::SourceName;
use borax_sources::store::{create_atomically, hash_file, write_atomically};
use toml_edit::{ArrayOfTables, DocumentMut, Item as TomlItem, Table, Value, value};
use uuid::Uuid;

use crate::config::OVERRIDE_FILE;
use crate::event::{
    AcceptedBy, AcceptedItemCause, Admission, Adoption, Condition, Diagnostic, Event, Extraction,
    Finding, Level, LibraryAnswer, Repair, RetrievedFrom,
};
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

/// The library's identity file, under [`STATE_DIR`].
pub const IDENTITY_FILE: &str = "library.toml";

/// Where the library rooted at `root` keeps its identity:
/// [`IDENTITY_FILE`] under [`STATE_DIR`].
pub fn identity_file(root: &Path) -> PathBuf {
    root.join(STATE_DIR).join(IDENTITY_FILE)
}

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
    below(&root, &path).is_some()
}

/// What remains of `path` after `root`, when `root` is a leading run of
/// its components, or `None` when it is not.
///
/// Components are matched the way the platform matches file names
/// ([`same_name`]), and what remains is `path`'s own components, as
/// `path` spells them. Both sides are taken as given: a caller wanting
/// `.` and `..` resolved normalises them first.
fn below<'a>(root: &Path, path: &'a Path) -> Option<Components<'a>> {
    let mut path = path.components();
    for expected in root.components() {
        let found = path.next()?;
        if !same_name(
            Path::new(expected.as_os_str()),
            Path::new(found.as_os_str()),
        ) {
            return None;
        }
    }
    Some(path)
}

/// `path` as a library-relative path: relative to `root` and
/// `/`-separated whatever the platform writes, which is the form an
/// artifact record stores.
///
/// `None` when `path` does not lie under `root`. Both sides are
/// normalised as [`contains`] normalises them, and their components
/// matched as it matches them, so a relative input and an absolute root
/// name one tree however each is spelled. The segments are `path`'s
/// own, spelled as `path` spells them.
pub fn library_relative(root: &Path, path: &Path) -> Option<String> {
    let (root, path) = normalised(root, path);
    let segments: Vec<String> = below(&root, &path)?
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(segments.join("/"))
}

/// `root` and `path` normalised lexically, each left as spelled where
/// it cannot be: a relative path with no working directory to resolve
/// it against.
fn normalised(root: &Path, path: &Path) -> (PathBuf, PathBuf) {
    (
        lexical(root).unwrap_or_else(|| root.to_path_buf()),
        lexical(path).unwrap_or_else(|| path.to_path_buf()),
    )
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
/// Both sides are normalised and compared as [`library_relative`]
/// normalises and compares them, so the answer does not depend on how
/// either is spelled.
pub fn excludes(root: &Path, path: &Path) -> bool {
    let (root, path) = normalised(root, path);
    let Some(relative) = below(&root, &path) else {
        return true;
    };

    let mut walked = root.clone();
    for component in relative {
        walked.push(component);
        if !owns(&root, &walked) {
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

/// A file or directory of a library's store, or the library's identity
/// file, that could not be read as what it claims to be.
///
/// One fault is one file: the store answers from everything else it
/// read, so an unreadable or unparsable file costs its own record and
/// no other. A fault whose path is the store's directory is a store
/// that could not be listed, in whole or in part, and costs every
/// record it would have listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreFault {
    /// The file or directory the fault is about.
    pub path: PathBuf,
    /// Why it could not be read, as the reader or the parser put it.
    pub message: String,
}

/// Every file of `directory` with the library's record extension, in
/// path order, each as its text or the fault reading it produced.
///
/// A directory that is not there yields nothing and no fault: a library
/// that has recorded nothing yet has no store to read. Any other
/// failure to list it yields one fault whose path is `directory`, and
/// so does an entry the listing could not produce. An entry whose
/// metadata cannot be read is a fault about that entry alone, as a
/// file that cannot be read is.
fn store_files(directory: &Path) -> Vec<(PathBuf, Result<String, StoreFault>)> {
    let fault = |path: &Path, error: io::Error| {
        (
            path.to_path_buf(),
            Err(StoreFault {
                path: path.to_path_buf(),
                message: error.to_string(),
            }),
        )
    };
    let listing = match fs::read_dir(directory) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![fault(directory, error)],
    };

    let mut files = Vec::new();
    for entry in listing {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                files.push(fault(directory, error));
                continue;
            }
        };
        let path = entry.path();
        let is_record = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(RECORD_EXTENSION));
        if !is_record {
            continue;
        }
        match entry.metadata() {
            Ok(metadata) if metadata.is_file() => {
                let text = fs::read_to_string(&path).map_err(|error| StoreFault {
                    path: path.clone(),
                    message: error.to_string(),
                });
                files.push((path, text));
            }
            Ok(_) => {}
            Err(error) => files.push(fault(&path, error)),
        }
    }
    files.sort_by(|(left, _), (right, _)| left.cmp(right));
    files
}

/// What a library's identity file says about the library's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    /// Nothing is at [`identity_file`]: the library has never been
    /// written to, or its identity file was deleted. A healthy state,
    /// and not a finding.
    Absent,
    /// The identity the file holds.
    Present(LibraryId),
    /// Something is at [`identity_file`] and cannot be read or does not
    /// parse as an identity. `path` is the identity file, and `message`
    /// is what the reader or the parser said.
    Faulty(StoreFault),
}

impl Identity {
    /// The identity, when there is one.
    pub fn id(&self) -> Option<&LibraryId> {
        match self {
            Identity::Present(id) => Some(id),
            Identity::Absent | Identity::Faulty(_) => None,
        }
    }
}

/// Read the identity of the library rooted at `root`. Writes nothing.
///
/// [`Identity::Absent`] only when nothing at all is at
/// [`identity_file`]. Anything there that cannot be read as an
/// identity is [`Identity::Faulty`]: a directory, a dangling symbolic
/// link, a file that cannot be read or is not UTF-8, text that does not
/// parse as a [`LibraryIdentity`], and a path whose metadata cannot be
/// read for any reason but its absence — a `.borax` that is a plain
/// file, say.
pub fn identity(root: &Path) -> Identity {
    let path = identity_file(root);
    let fault = |message: String| {
        Identity::Faulty(StoreFault {
            path: path.clone(),
            message,
        })
    };
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Identity::Absent,
        Err(error) => return fault(error.to_string()),
    }
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => return fault(error.to_string()),
    };
    match LibraryIdentity::from_toml(&text) {
        Ok(identity) => Identity::Present(identity.id),
        Err(malformed) => fault(malformed.to_string()),
    }
}

/// The identity of the library rooted at `root`, minted first if it has
/// none.
///
/// A library whose identity is [`Identity::Present`] or
/// [`Identity::Faulty`] is returned as it is, and nothing is written:
/// an identity file that cannot be read is never minted over. Otherwise
/// a v7 identity is minted and its file created through `create`, which
/// must never replace a file and must fail with
/// [`io::ErrorKind::AlreadyExists`] when something is already at the
/// path: [`store_create`] in a run. When it does fail that way, another
/// writer created the file first, and the identity is read again and
/// returned as that writer left it.
///
/// `Ok` is therefore never [`Identity::Absent`]. Fails when `create`
/// fails for any other reason, or when the file it found in its way is
/// gone by the time it is read again; the error has `create`'s kind and
/// says `cannot create the library's identity at <path>: <reason>`.
pub fn ensure_identity(
    root: &Path,
    create: &dyn Fn(&Path, &[u8]) -> io::Result<()>,
) -> io::Result<Identity> {
    let found = identity(root);
    if !matches!(found, Identity::Absent) {
        return Ok(found);
    }

    let path = identity_file(root);
    let id = LibraryId::from_uuid(Uuid::now_v7());
    let created = create(
        &path,
        LibraryIdentity { id: id.clone() }.to_toml().as_bytes(),
    );
    let error = match created {
        Ok(()) => return Ok(Identity::Present(id)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => match identity(root) {
            Identity::Absent => error,
            winner => return Ok(winner),
        },
        Err(error) => error,
    };
    Err(io::Error::new(
        error.kind(),
        format!(
            "cannot create the library's identity at {}: {error}",
            path.display()
        ),
    ))
}

/// Create `path` holding `bytes`, never replacing a file already there:
/// the `create` seam [`ensure_identity`] takes in a run.
///
/// Fails with [`io::ErrorKind::AlreadyExists`], leaving the file as it
/// is, when something is already at `path`. Where the filesystem can
/// hard-link, a reader sees no file or the whole of it; see
/// [`create_atomically`].
pub fn store_create(path: &Path, bytes: &[u8]) -> io::Result<()> {
    create_atomically(path, bytes)
}

/// The store writes a run makes into one library, each preceded by
/// ensuring the library has an identity.
///
/// Every write to the item store or the artifact store goes through
/// one, so a library's first store write is what mints its identity and
/// a run that writes nothing mints nothing. The identity is ensured
/// again before every write rather than once, so an identity file
/// deleted mid-run is minted again by the next write.
pub struct StoreWriter<'a> {
    root: &'a Path,
    create: &'a dyn Fn(&Path, &[u8]) -> io::Result<()>,
    write: &'a dyn Fn(&Path, &[u8]) -> io::Result<()>,
    fault: RefCell<Option<StoreFault>>,
}

impl<'a> StoreWriter<'a> {
    /// A writer into the library rooted at `root`, minting through
    /// `create` as [`ensure_identity`] does and writing through `write`,
    /// which replaces the file it writes: [`store_create`] and
    /// [`store_write`] in a run.
    pub fn new(
        root: &'a Path,
        create: &'a dyn Fn(&Path, &[u8]) -> io::Result<()>,
        write: &'a dyn Fn(&Path, &[u8]) -> io::Result<()>,
    ) -> StoreWriter<'a> {
        StoreWriter {
            root,
            create,
            write,
            fault: RefCell::new(None),
        }
    }

    /// Ensure the library's identity, then write `bytes` to `path`.
    ///
    /// When the identity cannot be minted, fails with
    /// [`ensure_identity`]'s error and does not write. An identity file
    /// that cannot be read neither stops the write nor is written over:
    /// the write is made, and the first such fault is kept for
    /// [`StoreWriter::identity_fault`]. Otherwise fails with what
    /// `write` said.
    pub fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if let Identity::Faulty(fault) = ensure_identity(self.root, self.create)? {
            self.fault.borrow_mut().get_or_insert(fault);
        }
        (self.write)(path, bytes)
    }

    /// The first [`Identity::Faulty`] identity a write met, if any.
    pub fn identity_fault(&self) -> Option<StoreFault> {
        self.fault.borrow().clone()
    }
}

/// What a run says about having written into a library whose identity
/// file could not be read, as [`StoreWriter::identity_fault`] kept it.
///
/// One warning however many writes met it: the file is the library's,
/// not any one record's. Nothing was refused on the strength of it and
/// nothing was written over it, so this is what the run has to say and
/// not why it did less; `borax validate` reports the file.
pub fn identity_fault_warning(fault: &StoreFault) -> Diagnostic {
    Diagnostic {
        level: Level::Warning,
        message: format!(
            "{} could not be read as the library's identity, so it was left as it is and the \
             library has none another location can name; borax validate reports why",
            fault.path.display()
        ),
    }
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

/// Whether `record` carries `identifier`: the field of `identifier`'s
/// own kind holds an identifier equal to it.
///
/// Both sides are in the parser's normal form, so a DOI compares
/// lowercased and without its resolver prefix, a PMID as an integer,
/// and an ISBN compact. An arXiv identifier is compared without its
/// version, since the record holds the version newest when it was
/// looked up and a file holds its own. An ISBN-10 and an ISBN-13 are
/// never taken for each other.
pub fn carries(record: &Record, identifier: &Identifier) -> bool {
    match (identifier, identifier_of(record, identifier)) {
        (Identifier::Arxiv(wanted), Some(Identifier::Arxiv(held))) => wanted.id() == held.id(),
        (_, held) => held.as_ref() == Some(identifier),
    }
}

/// The identifier an [`Extraction::Found`] names, read back from the
/// text it carries; `None` for every failure, and for text no
/// identifier parses from.
pub fn extracted_identifier(extraction: &Extraction) -> Option<Identifier> {
    match extraction {
        Extraction::Found { identifier, .. } => supplied(identifier).ok(),
        _ => None,
    }
}

/// What the changed bytes at `held`'s path are accepted on, or `None`
/// when they are not accepted.
///
/// [`AcceptedBy::Extracted`] when the item `held` links carries
/// `extracted`, the identifier extraction found in the bytes; else
/// [`AcceptedBy::Supplied`] when it carries `supplied`, the identifier
/// the operator supplied for the file. The linked item is the one
/// exactly one item of `items` carries: a record linking no item, or
/// an item no item or several items carry, carries nothing, so nothing
/// is accepted on its strength.
pub fn accepted_by(
    items: &ItemStore,
    held: &ArtifactRecord,
    extracted: Option<&Identifier>,
    supplied: Option<&Identifier>,
) -> Option<AcceptedBy> {
    let link = held.item.as_ref()?;
    let [(_, item)] = items.carrying(link).as_slice() else {
        return None;
    };
    let carried = |identifier: Option<&Identifier>| {
        identifier
            .filter(|identifier| carries(&item.record, identifier))
            .map(|identifier| identifier.to_string())
    };
    carried(extracted)
        .map(|identifier| AcceptedBy::Extracted { identifier })
        .or_else(|| carried(supplied).map(|identifier| AcceptedBy::Supplied { identifier }))
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
    ///
    /// `None` only for an item a previewing run learned it would mint,
    /// which no file holds; a store read from disk has a file for every
    /// item.
    entries: Vec<(Option<PathBuf>, Item)>,
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
                    store.entries.push((Some(path), parsed.item));
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

    /// The file the item of identity `id` was read from, or `None`
    /// when the store holds no such item.
    ///
    /// What a question about that item names, so an operator deciding
    /// about a work the library holds is shown the file that says what
    /// it is.
    pub fn file_of(&self, id: &ItemId) -> Option<&Path> {
        self.files()
            .find(|(_, item)| &item.id == id)
            .map(|(path, _)| path)
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

    /// Every item of identity `id`, each beside the file it came from
    /// (`None` for an item a preview learned it would mint), in the
    /// order they were read.
    fn carrying(&self, id: &ItemId) -> Vec<&(Option<PathBuf>, Item)> {
        self.entries
            .iter()
            .filter(|(_, held)| &held.id == id)
            .collect()
    }

    /// Every item read, each beside the file it came from, in the order
    /// they were read. An item no file holds is not among them.
    fn files(&self) -> impl Iterator<Item = (&Path, &Item)> {
        self.entries
            .iter()
            .filter_map(|(path, item)| Some((path.as_deref()?, item)))
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

/// What the library holds for an incoming file, as the two duplicate
/// checks ask it.
///
/// The four travel together because no one of them answers anything
/// alone: a record names a library-relative path, `root` is what makes
/// it a path on this machine, `items` is what a work is looked up in,
/// and `exists` is what says whether a recorded path still holds a
/// file. The stores are read once for a run and the account borrows
/// them, so a check costs no filesystem read beyond the one `exists`
/// makes.
///
/// An account is what `--no-record` withholds: a run told to keep no
/// account is given none, and makes neither check rather than making
/// both against an empty store.
pub struct Account<'a> {
    /// The library root the records' paths are relative to.
    pub root: &'a Path,
    /// The items a work is looked up among.
    pub items: &'a ItemStore,
    /// The records a hash is looked up among, and the link from an
    /// item back to the artifacts it has.
    pub records: &'a ArtifactStore,
    /// Whether a path still holds a file, answering as
    /// [`Path::exists`] does.
    ///
    /// Asked about a record that matched and about nothing else, so an
    /// answer of "not there" is exactly a record whose last-known path
    /// has gone stale — which is what a run reports as a library with
    /// paths to reconcile.
    pub exists: &'a dyn Fn(&Path) -> bool,
}

/// Whether `recorded`, a library-relative path under `root` as a record
/// stores one, names the file at `incoming`: the comparison
/// [`Account::is_incoming`] documents.
fn names(root: &Path, recorded: &str, incoming: &Path) -> bool {
    match (lexical(&relative_to(root, recorded)), lexical(incoming)) {
        (Some(recorded), Some(incoming)) => same_name(&recorded, &incoming),
        // With no working directory there is nothing to resolve a
        // relative path against, and nothing that can be said to be the
        // same file as another.
        _ => false,
    }
}

/// The work an incoming file turned out to be a second file of.
///
/// What a batch run reports and what an interactive run's question is
/// put from: the item is the work the library already holds, and
/// `existing` is the file the operator is deciding against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDuplicate {
    /// The item carrying one of the incoming file's identifiers.
    pub item: ItemId,
    /// The file that item was read from, as a path on this machine.
    /// `None` only for an item the store can no longer name a file
    /// for.
    pub item_file: Option<PathBuf>,
    /// A path recorded against that item which still holds a file: the
    /// artifact the incoming file would join, and the path the skip
    /// names. An item with no such path is no work duplicate at all,
    /// so this is never a path that holds nothing.
    pub existing: PathBuf,
}

impl Account<'_> {
    /// Whether the record recorded at `recorded` — a library-relative
    /// path, as a record stores it — is the record of the file at
    /// `incoming` rather than of another copy of it.
    ///
    /// The two are compared as paths and not as text: the record's
    /// path is resolved against the library root, the incoming path is
    /// made absolute against the working directory, and both are
    /// normalised lexically — `.` and `..` resolved without asking the
    /// filesystem — then matched as the platform matches file names,
    /// case-sensitively on Unix and case-insensitively on Windows.
    ///
    /// Symlinks are not resolved. A record names where borax last saw
    /// a file and the run names the path it was given; a link and its
    /// target are two names this does not try to unify, and treating
    /// them as one would mean a link into a library could keep the
    /// file it points at out of it.
    pub fn is_incoming(&self, recorded: &str, incoming: &Path) -> bool {
        names(self.root, recorded, incoming)
    }

    /// The record of the file at `incoming` itself, where the library
    /// has one: the record whose last-known path is that file's own.
    ///
    /// The one record neither check may report, since it records the
    /// file rather than another copy of it — and, through its item
    /// link and the item it answers with for the file's bytes, the
    /// items the work check may not report either.
    pub fn own_record(&self, incoming: &Path) -> Option<&ArtifactRecord> {
        self.records
            .iter()
            .find(|record| self.is_incoming(&record.path, incoming))
    }

    /// `recorded` as a path on this machine, or `None` when no file
    /// stands there.
    ///
    /// Disk decides: a record whose path has gone stale names no file
    /// to duplicate, and the account never vetoes an admission on the
    /// strength of one.
    fn live_path(&self, recorded: &str) -> Option<PathBuf> {
        let path = relative_to(self.root, recorded);
        (self.exists)(&path).then_some(path)
    }

    /// The file the library already holds with the same bytes as the
    /// file at `incoming`, whose hash is `hash`.
    ///
    /// A match is any record whose history holds `hash` — the whole
    /// history and not only its newest entry, so a file annotated
    /// since it was admitted is recognised as the artifact it is, and
    /// a copy of what an artifact used to be is recognised as a copy
    /// of it.
    ///
    /// The incoming file's own record is passed over and the search
    /// goes on, rather than the answer being discarded once found: a
    /// file whose own record answers first would otherwise hide a
    /// second copy recorded elsewhere in the library. A record whose
    /// last-known path holds no file is passed over on the same terms,
    /// and asking `exists` about it is what makes the run report a
    /// library with paths to reconcile.
    ///
    /// Answerable before the file is opened, which is the point: a
    /// byte-identical re-download is recognised without a single
    /// source being asked.
    pub fn content_duplicate(&self, incoming: &Path, hash: &ContentHash) -> Option<PathBuf> {
        self.records
            .by_hash(hash)
            .into_iter()
            .filter(|record| !self.is_incoming(&record.path, incoming))
            .find_map(|record| self.live_path(&record.path))
    }

    /// The work the library already holds a file for, among
    /// `identifiers`.
    ///
    /// The identifiers are tried in the order given, which is the
    /// order the caller ranks them by, and the first that answers
    /// decides. An identifier answers when an item carries it *and*
    /// that item has a record whose path still holds a file: an item
    /// with no artifact, and one whose every recorded artifact names a
    /// path holding nothing, is a work there is nothing to duplicate,
    /// so the incoming file is that item's first artifact rather than
    /// its second.
    ///
    /// The item the incoming file's own record already links to is
    /// passed over, and so is that record itself. Without the first
    /// rule every artifact of a multi-artifact item would meet its
    /// siblings as a duplicate of the item it belongs to, on every
    /// run, and a batch run would skip each of them. With `hash`, the
    /// incoming file's content hash, the item that record answers with
    /// for those bytes ([`ArtifactRecord::item_for`]) is passed over
    /// too: an earlier version restored at its path is that item's
    /// file. With `None`, only the link is.
    ///
    /// The artifacts an item has are the records linked to it, whatever
    /// bytes their files now hold, so a recorded path holding a
    /// restored version that answers with another item is still one of
    /// its link's artifacts.
    ///
    /// Answerable only once a record is in hand, which is the earliest
    /// a second PDF of one paper can be recognised at all.
    pub fn work_duplicate(
        &self,
        incoming: &Path,
        hash: Option<&ContentHash>,
        identifiers: &[Identifier],
    ) -> Option<WorkDuplicate> {
        let own = self.own_record(incoming);
        let link = own.and_then(|record| record.item.as_ref());
        let answered = own
            .zip(hash)
            .and_then(|(record, hash)| record.item_for(hash));
        identifiers.iter().find_map(|identifier| {
            self.items
                .iter()
                .filter(|item| identifier_of(&item.record, identifier).as_ref() == Some(identifier))
                .filter(|item| link != Some(&item.id) && answered != Some(&item.id))
                .find_map(|item| self.second_artifact_of(item, incoming))
        })
    }

    /// The work `item` is, as the duplicate an incoming file at
    /// `incoming` would be a second file of, or `None` when the library
    /// has no file of it to duplicate.
    ///
    /// The artifacts recorded against the item are tried in read order
    /// and the first whose path still holds a file decides. The
    /// incoming file's own record is passed over: a file is no second
    /// copy of itself.
    fn second_artifact_of(&self, item: &Item, incoming: &Path) -> Option<WorkDuplicate> {
        self.records
            .by_item(&item.id)
            .into_iter()
            .filter(|record| !self.is_incoming(&record.path, incoming))
            .find_map(|record| self.live_path(&record.path))
            .map(|existing| WorkDuplicate {
                item: item.id.clone(),
                item_file: self.items.file_of(&item.id).map(Path::to_path_buf),
                existing,
            })
    }
}

/// A library's two stores as one run holds them.
///
/// Read once, before the first file, and then kept current by the run
/// itself: each admission the run makes is learned as it is made
/// ([`Stores::learn`]), and each one a preview would make is learned
/// as it is planned ([`Stores::foresee`]). So every file is checked
/// against the library as the run has left it so far, a check costs no
/// directory walk, and a run of a hundred files reads the stores once.
/// [`Account`] is the same two stores with the run's `exists` beside
/// them, which is what a check is actually made against.
#[derive(Debug, Clone)]
pub struct Stores {
    /// The library root the records' paths are relative to.
    root: PathBuf,
    items: ItemStore,
    records: ArtifactStore,
}

impl Stores {
    /// Read both stores of the library rooted at `root`.
    ///
    /// `root` is normalised lexically first, and kept as spelled only
    /// where it cannot be — a relative root with no working directory —
    /// so every path the stores report is a full path however the root
    /// was spelled.
    ///
    /// Never fails, on [`ItemStore::read`]'s terms: a store directory
    /// that is not there is an empty store, so a directory borax has
    /// never written to reads as a library holding nothing, and one
    /// that cannot be listed is a [`StoreFault`] at that directory.
    pub fn read(root: &Path) -> Stores {
        let root = lexical(root).unwrap_or_else(|| root.to_path_buf());
        Stores {
            items: ItemStore::read(&root),
            records: ArtifactStore::read(&root),
            root,
        }
    }

    /// These stores as the account a check is made against, answering
    /// about a recorded path through `exists`.
    pub fn account<'a>(&'a self, exists: &'a dyn Fn(&Path) -> bool) -> Account<'a> {
        Account {
            root: &self.root,
            items: &self.items,
            records: &self.records,
            exists,
        }
    }

    /// What the library says about the file at `path`, whose content
    /// hash is `hash`, or `None` when the file lies outside the library
    /// or in a subtree it excludes ([`excludes`]).
    ///
    /// Root and path are compared normalised, as [`library_relative`]
    /// compares them, and a record names the file when
    /// [`Account::is_incoming`] would say it does. Only the records
    /// naming the file's path are examined; among them, the checks run
    /// in this order, the first that holds deciding:
    ///
    /// 1. None names it: [`LibraryAnswer::Untracked`], or
    ///    [`LibraryAnswer::UnreadableRecords`] when the artifact store
    ///    has faults ([`Stores::record_faults`]), since an unreadable
    ///    record might.
    /// 2. `hash` is `None`: [`LibraryAnswer::Unhashable`].
    /// 3. None holds `hash` anywhere in its history:
    ///    [`LibraryAnswer::UnrecognisedContent`].
    /// 4. Several hold it: [`LibraryAnswer::Ambiguous`].
    /// 5. The one that holds it links no item:
    ///    [`LibraryAnswer::NoItem`].
    /// 6. No item carries the linked identity:
    ///    [`LibraryAnswer::UnreadableItem`] when the item store's
    ///    directory, or an item file whose name claims that identity,
    ///    could not be read, and [`LibraryAnswer::DanglingItem`]
    ///    otherwise.
    /// 7. Several item files carry it: [`LibraryAnswer::AmbiguousItem`].
    /// 8. Exactly one does, and the item the record answers with for
    ///    `hash` ([`ArtifactRecord::item_for`]) is the link:
    ///    [`LibraryAnswer::Tracked`], with that item.
    /// 9. It answers with another item, which the bytes were accepted
    ///    under: [`LibraryAnswer::Tracked`] naming it as
    ///    `accepted_under`, with that item, when exactly one item file
    ///    carries it, and [`LibraryAnswer::AcceptedItemUnavailable`]
    ///    otherwise.
    ///
    /// Steps 5 to 7 are about the link whichever entry of the history
    /// the file's bytes are, so a damaged link is reported as damaged
    /// even where an earlier entry's item could have answered.
    ///
    /// Reads nothing from disk beyond what [`excludes`] asks about
    /// nested libraries: the answer is the stores as they were read.
    pub fn consult(&self, path: &Path, hash: Option<&ContentHash>) -> Option<Consulted> {
        if !self.covers(path) {
            return None;
        }
        let answer = |answer: LibraryAnswer| Some(Consulted { answer, item: None });
        let ids = |records: &[&ArtifactRecord]| -> Vec<String> {
            records.iter().map(|record| record.id.to_string()).collect()
        };

        let at_path: Vec<&ArtifactRecord> = self
            .records
            .iter()
            .filter(|record| names(&self.root, &record.path, path))
            .collect();
        if at_path.is_empty() {
            return answer(match self.record_faults() {
                Some(RecordFaults { listed, unreadable }) => {
                    LibraryAnswer::UnreadableRecords { listed, unreadable }
                }
                None => LibraryAnswer::Untracked,
            });
        }
        let Some(hash) = hash else {
            return answer(LibraryAnswer::Unhashable {
                artifacts: ids(&at_path),
            });
        };
        let holding: Vec<&ArtifactRecord> = at_path
            .iter()
            .copied()
            .filter(|record| record.holds(hash))
            .collect();
        let record = match holding.as_slice() {
            [] => {
                return answer(LibraryAnswer::UnrecognisedContent {
                    artifacts: ids(&at_path),
                });
            }
            [record] => *record,
            _ => {
                return answer(LibraryAnswer::Ambiguous {
                    artifacts: ids(&holding),
                });
            }
        };
        let artifact = record.id.to_string();
        let Some(linked) = &record.item else {
            return answer(LibraryAnswer::NoItem { artifact });
        };
        let item = linked.to_string();

        let files = |several: &[&(Option<PathBuf>, Item)]| -> Vec<PathBuf> {
            several
                .iter()
                .filter_map(|(file, _)| file.clone())
                .collect()
        };

        let held = match self.items.carrying(linked).as_slice() {
            [] => {
                return answer(match self.item_fault(linked) {
                    Some(fault) => LibraryAnswer::UnreadableItem {
                        artifact,
                        item,
                        path: fault.path.clone(),
                        message: fault.message.clone(),
                    },
                    None => LibraryAnswer::DanglingItem { artifact, item },
                });
            }
            [(_, held)] => held.clone(),
            several => {
                return answer(LibraryAnswer::AmbiguousItem {
                    artifact,
                    item,
                    files: files(several),
                });
            }
        };
        let accepted = match record.item_for(hash) {
            Some(accepted) if accepted != linked => accepted,
            _ => {
                return Some(Consulted {
                    answer: LibraryAnswer::Tracked {
                        artifact,
                        item,
                        accepted_under: None,
                    },
                    item: Some(held),
                });
            }
        };
        let accepted_under = accepted.to_string();
        let cause = match self.items.carrying(accepted).as_slice() {
            [(_, held)] => {
                return Some(Consulted {
                    answer: LibraryAnswer::Tracked {
                        artifact,
                        item,
                        accepted_under: Some(accepted_under),
                    },
                    item: Some(held.clone()),
                });
            }
            [] => match self.item_fault(accepted) {
                Some(fault) => AcceptedItemCause::Unreadable {
                    path: fault.path.clone(),
                    message: fault.message.clone(),
                },
                None => AcceptedItemCause::Dangling,
            },
            several => AcceptedItemCause::Ambiguous {
                files: files(several),
            },
        };
        answer(LibraryAnswer::AcceptedItemUnavailable {
            artifact,
            item,
            accepted_under,
            cause,
        })
    }

    /// Whether the file at `path` is one [`Stores::consult`] answers
    /// about: it lies under the library root, compared normalised, and
    /// in no subtree the library excludes.
    pub fn covers(&self, path: &Path) -> bool {
        library_relative(&self.root, path).is_some() && !excludes(&self.root, path)
    }

    /// The item-store fault that may have cost the item `id`: the
    /// store's directory itself, else the first file whose name claims
    /// that identity. `None` when no fault can be attributed to it.
    fn item_fault(&self, id: &ItemId) -> Option<&StoreFault> {
        let directory = self.root.join(ITEM_STORE);
        let faults = &self.items.faults;
        faults
            .iter()
            .find(|fault| fault.path == directory)
            .or_else(|| {
                faults.iter().find(|fault| {
                    fault
                        .path
                        .file_name()
                        .is_some_and(|name| name_uuid(&name.to_string_lossy()) == Some(id.uuid()))
                })
            })
    }

    /// What the artifact store could not read, or `None` when it was
    /// read whole, an absent store included.
    ///
    /// `listed` is `false` when the store's directory could not be
    /// listed, in whole or in part, and `unreadable` then 0. Otherwise
    /// `unreadable` counts the record files that could not be read or
    /// parsed.
    pub fn record_faults(&self) -> Option<RecordFaults> {
        let faults = &self.records.faults;
        if faults.is_empty() {
            return None;
        }
        let directory = self.root.join(STATE_DIR).join(ARTIFACT_STORE);
        Some(match faults.iter().any(|fault| fault.path == directory) {
            true => RecordFaults {
                listed: false,
                unreadable: 0,
            },
            false => RecordFaults {
                listed: true,
                unreadable: faults.len(),
            },
        })
    }

    /// Take in what [`admit`] wrote for `admitting`, which it reported
    /// as `admitted`, so every later check answers as if the stores had
    /// been read after the write.
    ///
    /// The artifact record of identity `admitted.artifact` names
    /// `admitting.path`, links to `admitted.item`, and holds
    /// `admitting.hash` in its history, appended on top of the history
    /// `admitting.held` had on [`admit`]'s terms: it replaces the record
    /// of that identity wherever it stood, which is what finds a moved
    /// artifact at its new path and no longer at its old one. An item the stores do not
    /// hold is taken as the one `admit` minted, carrying
    /// `admitting.record` and held in the file `admit` named for it.
    ///
    /// The record's size and modification time are `admitting.held`'s,
    /// or zero for a record minted here: no check reads them. Nothing
    /// is learned for a path outside the library, which `admit` records
    /// nothing for.
    pub fn learn(&mut self, admitting: &Admitting<'_>, admitted: &Admitted) {
        let file = self
            .root
            .join(ITEM_STORE)
            .join(item_file_name(admitting.key, &admitted.item));
        self.take_in(admitting, admitted, Some(file));
    }

    /// Take in what [`admit`] would write for `admitting`, without
    /// writing anything, and report it as `admit` would.
    ///
    /// The item is selected as [`admit`] selects it — the held
    /// record's link unless the file was re-identified or the link is
    /// absent, then the first item carrying one of the record's
    /// identifiers — but among these stores rather than a fresh read.
    /// Where none answers, an item is minted from `admitting.record`
    /// under a fresh identity and learned with no file, since none will
    /// be written. The artifact record is the held one's identity, or a
    /// fresh one, and is learned on [`Stores::learn`]'s terms.
    ///
    /// `None`, learning nothing, for a path outside the library.
    pub fn foresee(&mut self, admitting: &Admitting<'_>) -> Option<Admitted> {
        library_relative(&self.root, admitting.path)?;
        let kept = admitting
            .held
            .and_then(|held| held.item.clone())
            .filter(|_| !admitting.reidentified);
        let item = kept
            .or_else(|| {
                identifiers(admitting.record)
                    .iter()
                    .find_map(|identifier| self.items.by_identifier(identifier))
                    .map(|item| item.id.clone())
            })
            .unwrap_or_else(|| ItemId::from_uuid(Uuid::now_v7()));
        let admitted = Admitted {
            artifact: admitting.held.map_or_else(
                || ArtifactId::from_uuid(Uuid::now_v7()),
                |held| held.id.clone(),
            ),
            relinked_from: admitting
                .held
                .and_then(|held| held.item.clone())
                .filter(|before| before != &item),
            item,
        };
        self.take_in(admitting, &admitted, None);
        Some(admitted)
    }

    /// Learn the artifact record `admitted` describes and, when the
    /// stores do not hold its item yet, that item as held in
    /// `item_file`.
    fn take_in(
        &mut self,
        admitting: &Admitting<'_>,
        admitted: &Admitted,
        item_file: Option<PathBuf>,
    ) {
        let Some(relative) = library_relative(&self.root, admitting.path) else {
            return;
        };

        if self.items.by_id(&admitted.item).is_none() {
            self.items.entries.push((
                item_file,
                Item {
                    id: admitted.item.clone(),
                    record: admitting.record.clone(),
                },
            ));
        }

        let mut record = admitting.held.cloned().unwrap_or_else(|| ArtifactRecord {
            id: admitted.artifact.clone(),
            item: None,
            path: String::new(),
            size: 0,
            modified_millis: 0,
            history: Vec::new(),
        });
        let link_as_read = record.item.clone();
        record.id = admitted.artifact.clone();
        record.item = Some(admitted.item.clone());
        record.path = relative;
        settle_history(&mut record, admitting, link_as_read.as_ref());

        match self
            .records
            .entries
            .iter_mut()
            .find(|(_, held)| held.id == record.id)
        {
            Some((_, held)) => *held = record,
            None => {
                let file = self
                    .root
                    .join(STATE_DIR)
                    .join(ARTIFACT_STORE)
                    .join(format!("{}.{RECORD_EXTENSION}", record.id));
                self.records.entries.push((file, record));
            }
        }
    }
}

/// Append `admitting.hash` to `record`'s history where the admission
/// decides something about those bytes, after `record`'s link has been
/// set to the item it links once the admission is made.
///
/// `link_as_read` is the link the record carried as it was read,
/// before the admission assigned one. Bytes the history already holds
/// are appended only when the admission decided which item they belong
/// to — a re-identification, or a record that linked no item gaining
/// one — and are not already the newest entry answering with that item
/// and carrying the admission's evidence. So a re-identification of
/// restored bytes appends, and so does one of the newest bytes whose
/// evidence the newest entry does not already carry; the same decision
/// made again appends nothing. Bytes the history does not hold are
/// always appended, the caller having accepted them. The entry names
/// the record's link and carries [`Admitting::evidence`], and
/// [`ArtifactRecord::append`] restates the newest bytes first where the
/// link as read was corrected by hand.
fn settle_history(
    record: &mut ArtifactRecord,
    admitting: &Admitting<'_>,
    link_as_read: Option<&ItemId>,
) {
    let held = record.holds(admitting.hash);
    let decided = admitting.reidentified || link_as_read.is_none();
    let answered = record.current_hash() == Some(admitting.hash)
        && record.item_for(admitting.hash) == record.item.as_ref()
        && admitting.evidence.is_none_or(|evidence| {
            record
                .history
                .last()
                .and_then(|newest| newest.evidence.as_ref())
                == Some(evidence)
        });
    if held && (!decided || answered) {
        return;
    }
    let entry = HashEntry {
        hash: admitting.hash.clone(),
        run: admitting.run.clone(),
        timestamp: admitting.timestamp.to_string(),
        tool_version: admitting.tool_version.to_string(),
        item: record.item.clone(),
        evidence: admitting.evidence.cloned(),
    };
    record.append(entry, link_as_read);
}

/// What a library said about one file, and the item it answered with.
#[derive(Debug, Clone, PartialEq)]
pub struct Consulted {
    pub answer: LibraryAnswer,
    /// The item answered with: `Some` exactly when `answer` is
    /// [`LibraryAnswer::Tracked`]. That is the item named as its
    /// `accepted_under` when it names one, and the linked item
    /// otherwise.
    pub item: Option<Item>,
}

/// What a library's artifact store could not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordFaults {
    /// Whether the store's directory could be listed.
    pub listed: bool,
    /// How many record files could not be read or parsed; 0 when the
    /// directory could not be listed.
    pub unreadable: usize,
}

/// What a run says about having matched a record whose artifact is no
/// longer where the record says.
///
/// One warning however many records turned out to be stale: they are
/// all the same fact about the library, and the remedy is the same
/// reconcile whether one path is out of date or a hundred. Nothing was
/// refused on the strength of one — disk is the source of truth — so
/// this is what the run has to say and not why it did less.
pub fn stale_paths_warning() -> Diagnostic {
    Diagnostic {
        level: Level::Warning,
        message: "the library records artifacts that are no longer where it says, so it holds \
                  paths to reconcile; borax reconcile repairs them"
            .to_string(),
    }
}

/// What a run says about consulting a library whose artifact store it
/// could not read whole, as [`Stores::record_faults`] described it.
///
/// One warning however many files the run consulted: the fault is the
/// store's, and each file it may have cost says so on its own event.
/// Nothing was refused on the strength of it, so this is what the run
/// has to say and not why it did less; `borax validate` names the files.
pub fn record_faults_warning(faults: RecordFaults) -> Diagnostic {
    let message = match faults {
        RecordFaults { listed: false, .. } => {
            "the library's artifact records could not be listed, so no file is resolved from \
             the library; borax validate reports why"
                .to_string()
        }
        RecordFaults { unreadable: 1, .. } => {
            "1 artifact record could not be read, so a file it records is resolved as if the \
             library did not track it; borax validate names it"
                .to_string()
        }
        RecordFaults { unreadable, .. } => format!(
            "{unreadable} artifact records could not be read, so a file one of them records is \
             resolved as if the library did not track it; borax validate names them"
        ),
    };
    Diagnostic {
        level: Level::Warning,
        message,
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

/// The records of `records` whose artifact `exists` does not find, in
/// read order.
///
/// `exists` answers whether the library has an artifact at a path, and
/// the caller decides what that takes: `borax validate` asks for a file
/// in a subtree the library owns, so a file under a nested library
/// counts as no artifact of this one.
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
/// kept rather than only their totals: a caller asked for more — each
/// artifact's extraction result, and the identifiable count taken over
/// those results — needs the artifacts themselves, and a walk is the
/// one part of the survey worth not doing twice.
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
    /// holds them: the objects behind the `orphans` count, each named by
    /// one of the events [`orphan_events`] gives.
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
    /// The library's identity, when its identity file holds a readable
    /// one: `None` both for a library with no identity file and for one
    /// whose file cannot be read, which is [`validate`]'s to report.
    pub identity: Option<LibraryId>,
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

/// A library as read: one walk of its tree, one read of each of its
/// stores and one of its identity, which is everything [`survey`] and
/// [`validate`] are made of.
struct Contents {
    artifacts: Vec<PathBuf>,
    nested: Vec<String>,
    items: ItemStore,
    records: ArtifactStore,
    identity: Identity,
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
        identity: identity(root),
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
        identity: contents.identity.id().cloned(),
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

/// The `library-extraction` event reporting `extraction` for the
/// surveyed artifact at `artifact`, named relative to `survey.root`.
///
/// The path is `/`-separated on every platform, as an artifact record
/// stores one. An artifact the survey found always lies under its
/// root; one that does not is named as `artifact` spells it, with the
/// platform's separator written as `/`.
pub fn extraction_event(survey: &Survey, artifact: &Path, extraction: Extraction) -> Event {
    Event::LibraryExtraction {
        path: reported(&survey.root, artifact),
        extraction,
    }
}

/// The `library-condition` events naming each orphan of `survey`, in
/// survey order, each named relative to `survey.root` as
/// [`extraction_event`] names an artifact.
///
/// One event per entry of [`Survey::orphans`], so their number is the
/// `orphans` count [`status_event`] reports.
pub fn orphan_events(survey: &Survey) -> Vec<Event> {
    survey
        .orphans
        .iter()
        .map(|orphan| Event::LibraryCondition {
            path: reported(&survey.root, orphan),
            condition: Condition::Orphan,
        })
        .collect()
}

/// `path` as a run reports a file of the library rooted at `root`:
/// library-relative and `/`-separated, or, for a path not under `root`,
/// as `path` spells it with the platform's separator written as `/`.
fn reported(root: &Path, path: &Path) -> String {
    library_relative(root, path).unwrap_or_else(|| {
        path.display()
            .to_string()
            .replace(std::path::MAIN_SEPARATOR, "/")
    })
}

/// The event reporting `survey`, carrying `identifiable` as the count
/// of artifacts an identifier could be extracted from, and the
/// library's identity when the survey found one.
///
/// Its `orphans` count is the number of events [`orphan_events`] gives
/// for the same survey.
///
/// `identifiable` is `None` when the run was not asked for it: the
/// survey itself never opens a document, so the count comes from the
/// caller that did, taken over the per-artifact results it reported
/// through [`extraction_event`] so that the two cannot disagree.
pub fn status_event(survey: &Survey, identifiable: Option<usize>) -> Event {
    Event::LibraryStatus {
        root: survey.root.clone(),
        id: survey.identity.as_ref().map(LibraryId::to_string),
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
/// conditions are not findings and never become them: an orphan is
/// work to do, an artifact borax cannot find is history the library
/// deliberately keeps, and an item nothing links to is an ordinary item
/// for a work with no file. A record or item can carry a finding and be
/// a condition at once; the two reports are independent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validation {
    /// The library the findings are about, as [`Survey::root`].
    pub root: PathBuf,
    /// Every finding, each with the file it is about, in the order the
    /// library was read: the identity file first, then the item store,
    /// then the artifact records, each store in path order. An identity
    /// file that cannot be read or does not parse is one
    /// [`Finding::Unreadable`]; an absent one is no finding.
    pub findings: Vec<(PathBuf, Finding)>,
    /// Every condition, each with the library-relative path it is
    /// about: the orphans in survey order, then the missing records in
    /// read order, then the unlinked items in read order.
    ///
    /// An orphan is an artifact no record names. A missing record is
    /// one whose last-known path holds no file, or holds one in a
    /// subtree the library does not own, which it can no more see than
    /// an absent file; its path is the one it records. An unlinked
    /// item is one no artifact record links to, named by its file.
    pub conditions: Vec<(String, Condition)>,
    /// The library's identity, as [`Survey::identity`].
    pub identity: Option<LibraryId>,
}

impl Validation {
    /// How many of [`Validation::conditions`] are orphans.
    pub fn orphans(&self) -> usize {
        self.count(|condition| matches!(condition, Condition::Orphan))
    }

    /// How many of [`Validation::conditions`] are missing records.
    pub fn missing(&self) -> usize {
        self.count(|condition| matches!(condition, Condition::Missing { .. }))
    }

    /// How many of [`Validation::conditions`] are unlinked items.
    pub fn unlinked(&self) -> usize {
        self.count(|condition| matches!(condition, Condition::Unlinked { .. }))
    }

    /// How many of [`Validation::conditions`] `kind` accepts.
    fn count(&self, kind: impl Fn(&Condition) -> bool) -> usize {
        self.conditions
            .iter()
            .filter(|(_, condition)| kind(condition))
            .count()
    }
}

/// Validate the library rooted at `root`.
///
/// Reads the tree, both stores and the identity file, and reports what
/// it found. Repairs nothing, refuses nothing, writes nothing — an
/// identity included — and holds no lock: a
/// library has no single consistent state at any moment, its writers
/// being peers, so a file observed half-written is a finding about that
/// file and about nothing else.
///
/// A record counts as missing when its path, resolved as [`missing`]
/// resolves one, holds no file in a subtree the library owns.
pub fn validate(root: &Path) -> Validation {
    let contents = contents(root);
    let survey = surveyed(root, &contents);

    let mut findings: Vec<(PathBuf, Finding)> = match &contents.identity {
        Identity::Faulty(fault) => vec![(
            fault.path.clone(),
            Finding::Unreadable {
                message: fault.message.clone(),
            },
        )],
        Identity::Absent | Identity::Present(_) => Vec::new(),
    };
    findings.extend(item_findings(&contents.items));
    findings.extend(record_findings(&contents.records, &contents.items));

    let exists = |path: &Path| !excludes(root, path) && path.is_file();
    let orphaned = survey
        .orphans
        .iter()
        .map(|orphan| (reported(root, orphan), Condition::Orphan));
    let missing = contents
        .records
        .files()
        .filter(|(_, record)| !exists(&relative_to(root, &record.path)))
        .map(|(file, record)| {
            let condition = Condition::Missing {
                id: record.id.to_string(),
                record: reported(root, file),
            };
            (record.path.clone(), condition)
        });
    let unlinked = contents
        .items
        .files()
        .filter(|(_, item)| contents.records.by_item(&item.id).is_empty())
        .map(|(file, item)| {
            let condition = Condition::Unlinked {
                id: item.id.to_string(),
            };
            (reported(root, file), condition)
        });

    Validation {
        conditions: orphaned.chain(missing).chain(unlinked).collect(),
        root: survey.root,
        identity: survey.identity,
        findings,
    }
}

/// The events reporting `validation`: one per finding in the order they
/// were found, then one per condition in the order
/// [`Validation::conditions`] holds them, then the totals.
///
/// The totals come last so that a reader of the stream has every
/// finding and condition before the count of them, as every other run
/// reports its files before its summary. Each condition count is the
/// number of condition events of its kind, and the totals carry the
/// library's identity when it has one.
pub fn validation_events(validation: &Validation) -> Vec<Event> {
    let findings = validation
        .findings
        .iter()
        .map(|(path, finding)| Event::LibraryFinding {
            path: path.clone(),
            finding: finding.clone(),
        });
    let conditions =
        validation
            .conditions
            .iter()
            .map(|(path, condition)| Event::LibraryCondition {
                path: path.clone(),
                condition: condition.clone(),
            });
    let mut events: Vec<Event> = findings.chain(conditions).collect();

    events.push(Event::LibraryValidated {
        root: validation.root.clone(),
        id: validation.identity.as_ref().map(LibraryId::to_string),
        findings: validation.findings.len(),
        orphans: validation.orphans(),
        missing: validation.missing(),
        unlinked: validation.unlinked(),
    });
    events
}

/// What the filesystem says about a file, as an artifact record records
/// it: its size in bytes and its modification time in milliseconds
/// since the Unix epoch.
///
/// `None` when the file cannot be stat'd, or when the platform will not
/// say when it was modified. A modification time before the epoch is
/// negative, which is why the field is signed.
fn seen(path: &Path) -> Option<(u64, i64)> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let millis = match modified.duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_millis()).ok()?,
        Err(before) => -i64::try_from(before.duration().as_millis()).ok()?,
    };
    Some((metadata.len(), millis))
}

/// Whether `record` describes the file at `path` as it stands: the size
/// and the modification time the record holds are the file's.
///
/// This is the fast path, and it is what a reconcile compares before it
/// hashes anything. It misses a change within the granularity the
/// recorded modification time keeps, which is what `--rehash` is for.
/// A file that cannot be stat'd matches nothing.
fn describes(record: &ArtifactRecord, path: &Path) -> bool {
    seen(path) == Some((record.size, record.modified_millis))
}

/// `entry` as the table an artifact record writes a history entry as.
/// An entry naming no item is written with no `item` key, and one
/// carrying no evidence with neither `identifier` nor `origin`; the
/// evidence's two keys follow `item`.
fn history_table(entry: &HashEntry) -> Table {
    let mut table = Table::new();
    table["hash"] = value(entry.hash.as_str());
    table["run"] = value(entry.run.as_str());
    table["timestamp"] = value(entry.timestamp.as_str());
    table["tool_version"] = value(entry.tool_version.as_str());
    if let Some(item) = &entry.item {
        table["item"] = value(item.to_string());
    }
    if let Some(evidence) = &entry.evidence {
        table["identifier"] = value(evidence.identifier.to_string());
        table["origin"] = value(evidence.origin.as_str());
    }
    table
}

/// Add to `document`'s history every entry of `history` beyond the ones
/// it already holds, in order.
///
/// A history only ever grows, so the entries past the length the
/// document carries are the ones to add, and the entries already
/// written are left exactly as they are. Both spellings of an array are
/// appended to in their own spelling, so a record written as tables
/// stays tables.
///
/// Fails when `history` is something other than an array, which is a
/// record the parser would not have accepted.
fn append_history(document: &mut DocumentMut, history: &[HashEntry]) -> io::Result<()> {
    let written = match document.get("history") {
        Some(TomlItem::ArrayOfTables(tables)) => tables.len(),
        Some(TomlItem::Value(Value::Array(array))) => array.len(),
        Some(_) => return Err(io::Error::other("history is not an array")),
        None => 0,
    };
    let Some(added) = history.get(written..) else {
        return Ok(());
    };
    if added.is_empty() {
        return Ok(());
    }

    match document
        .entry("history")
        .or_insert_with(|| TomlItem::ArrayOfTables(ArrayOfTables::new()))
    {
        TomlItem::ArrayOfTables(tables) => {
            for entry in added {
                tables.push(history_table(entry));
            }
            Ok(())
        }
        TomlItem::Value(Value::Array(array)) => {
            for entry in added {
                array.push(history_table(entry).into_inline_table());
            }
            Ok(())
        }
        _ => Err(io::Error::other("history is not an array")),
    }
}

/// Write `record` back to `path`, leaving the rest of the document as
/// it was.
///
/// The file's text is parsed as a document, the fields `record` now
/// carries are set on it, and the result is rendered: an artifact
/// record's key order, its formatting and any comment a person left in
/// it survive an edit to one field. A rendering identical to what the
/// file already holds is not written at all, which is what leaves a
/// reconcile that decided nothing with no diff and no touched
/// modification time.
///
/// The write goes through `write`, which in a run is a whole-file
/// atomic replacement ([`store_write`]): a reader either sees the
/// record as it was or the record as it now is, and an interrupted
/// write leaves the previous one intact.
///
/// The item link is set only when `record` names an item other than
/// the one the document names, so a write that leaves the link where
/// it was leaves its spelling alone too. A record naming no item never
/// removes one from the document.
///
/// Fails with what the filesystem said, which the caller reports as
/// [`Repair::Unwritten`] or [`Admission::Unwritten`]. A file that
/// cannot be read or does not parse is such a failure: this rewrites a
/// record borax read, and nothing here creates one.
fn rewrite(
    path: &Path,
    record: &ArtifactRecord,
    write: &dyn Fn(&Path, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let text = fs::read_to_string(path)?;
    let mut document: DocumentMut = text.parse().map_err(io::Error::other)?;

    if let Some(item) = &record.item {
        let linked = document.get("item").and_then(TomlItem::as_str);
        if linked != Some(item.to_string().as_str()) {
            document["item"] = value(item.to_string());
        }
    }
    document["path"] = value(record.path.as_str());
    document["size"] = value(i64::try_from(record.size).map_err(io::Error::other)?);
    document["modified_millis"] = value(record.modified_millis);
    append_history(&mut document, &record.history)?;

    let rendered = document.to_string();
    match rendered == text {
        true => Ok(()),
        false => write(path, rendered.as_bytes()),
    }
}

/// Set `record`'s size and modification time to the file it now names.
///
/// Leaves both as they are when that file cannot be stat'd: a record
/// whose fields borax could not read is no worse off than before.
fn refresh(root: &Path, record: &mut ArtifactRecord) {
    if let Some((size, modified_millis)) = seen(&relative_to(root, &record.path)) {
        record.size = size;
        record.modified_millis = modified_millis;
    }
}

/// The hashing one reconcile did, memoised by library-relative path.
///
/// A file is read once however many records ask about it, which is what
/// makes the count of artifacts hashed the count of artifacts rather
/// than the count of questions.
#[derive(Debug, Default)]
struct Hashes {
    /// Every path asked about, with what it yielded — `None` for a file
    /// that could not be read, which is asked about no more often than
    /// one that could.
    computed: BTreeMap<String, Option<ContentHash>>,
    /// How many artifacts were hashed. A file that could not be read
    /// was not one of them.
    hashed: usize,
}

impl Hashes {
    /// The hash of the file at `relative` under `root`, or `None` when
    /// it cannot be read — which a path holding no file is.
    fn of(&mut self, root: &Path, relative: &str) -> Option<ContentHash> {
        if let Some(hash) = self.computed.get(relative) {
            return hash.clone();
        }

        let hash = hash_file(&relative_to(root, relative)).ok();
        if hash.is_some() {
            self.hashed += 1;
        }
        self.computed.insert(relative.to_string(), hash.clone());
        hash
    }
}

/// What a reconcile's four steps decided about one record, before
/// anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Decided {
    /// The record's own path holds its artifact. `refreshed` is set
    /// when the fast path missed and a hash in the record's history
    /// confirmed it: the bytes are right and the recorded size and
    /// modification time are not.
    Confirmed { refreshed: bool },
    /// The artifact was found at this library-relative path.
    Repaired { to: String },
    /// The artifact at the record's own path changed since borax last
    /// saw it, now hashes to `hash`, and was accepted on `accepted`.
    Changed {
        hash: ContentHash,
        accepted: AcceptedBy,
    },
    /// The artifact at the record's own path changed since borax last
    /// saw it, now hashes to `hash`, and was not accepted: extraction
    /// made `extraction` of it.
    Pending {
        hash: ContentHash,
        extraction: Extraction,
    },
    /// Nothing was decided.
    Unresolved,
}

/// How one reconcile is to run: what it hashes regardless, what the
/// operator accepts, how it reads a document, and what it stamps an
/// entry with.
pub struct Reconciling<'a> {
    /// Whether to hash every record's file whatever the fast path says.
    pub rehash: bool,
    /// The `--accept` paths, as given: each taken as every path
    /// argument is, made absolute against the working directory and
    /// normalised lexically. A record whose last-known path is one of
    /// them is hashed whatever the fast path says, and its pending
    /// change is accepted.
    pub accept: &'a [PathBuf],
    /// What extraction makes of the file at a full path. Asked only
    /// about a file whose bytes changed at a recorded path.
    pub extract: &'a dyn Fn(&Path) -> Extraction,
    /// The run an appended entry is stamped with.
    pub run: RunId,
    /// The timestamp an appended entry is stamped with.
    pub timestamp: &'a str,
    /// The borax version an appended entry is stamped with.
    pub tool_version: &'a str,
}

/// What a reconcile made of a library.
///
/// The three counts are about the run as a whole; `repairs` is what it
/// has to say about individual records, and holds nothing about a
/// record it merely confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciliation {
    /// The library reconciled, as [`Survey::root`].
    pub root: PathBuf,
    /// Every record the run had something to say about: its identity,
    /// where its artifact stands after the run, and what was made of
    /// it. In the order the store was read, which is path order.
    pub repairs: Vec<(ArtifactId, String, Repair)>,
    /// How many records the artifact store holds. A file under
    /// `.borax/artifacts/` that does not parse is not one of them and
    /// is not reconciled; `borax validate` is what names it.
    pub records: usize,
    /// Records whose artifact was where the record said it would be —
    /// by the fast path, or by a hash in the record's own history after
    /// the fast path missed.
    pub confirmed: usize,
    /// Records whose change is pending: one [`Repair::Pending`] each in
    /// `repairs`.
    pub pending: usize,
    /// How many artifacts the run hashed.
    pub hashed: usize,
    /// The `--accept` paths, as given and in the order given, at which
    /// no change was pending to accept: each one the run reports as a
    /// `not-pending` skip. A path given twice is listed once.
    pub not_pending: Vec<PathBuf>,
    /// The library's identity file as the run's first write found it
    /// unreadable, when it did: the fault
    /// [`identity_fault_warning`] reports. `None` for a run that wrote
    /// nothing, whatever the file holds.
    pub identity_fault: Option<StoreFault>,
}

/// Reconcile the library rooted at `root`, writing what it repairs.
///
/// Brings each artifact record's last-known path back into agreement
/// with the tree. Every record is compared against the file it names by
/// size and modification time first, and only what that comparison
/// leaves unsettled is hashed; [`Reconciling::rehash`] skips the
/// comparison and hashes regardless, which is what finds a change made
/// within the granularity a recorded modification time keeps. A record
/// whose last-known path is named in [`Reconciling::accept`] is hashed
/// regardless too.
///
/// Records are resolved in four steps, each run to completion across
/// every record before the next begins:
///
/// 1. **Settled.** A record whose last-known path holds a file matching
///    the fast path, or holding any hash in that record's history, is
///    confirmed, and that artifact is *claimed* by it.
/// 2. **Repair by content.** Each remaining record is matched against
///    the artifacts no record claimed, by any hash in its history, a
///    match on the record's current hash preferred over one on an
///    earlier hash. A record with exactly one candidate that no other
///    record also wants has its path repaired and claims that artifact.
/// 3. **Changed in place.** A record still unresolved whose last-known
///    path holds an unclaimed artifact matching no record's history has
///    changed since borax last saw it, and claims that artifact whatever
///    is decided about the change. Extraction is asked about the file:
///    the change is accepted when the identifier it finds is one the
///    record's linked item carries ([`accepted_by`]), or else when the
///    operator named the path, and its hash is then appended after the
///    hashes already recorded, naming the record's link. Otherwise the
///    change is pending and nothing is written for the record, its size
///    and modification time included, so the next reconcile meets it
///    again.
/// 4. **Unresolved.** Everything still unresolved is left exactly as it
///    is, and reported: [`Repair::Ambiguous`] naming its candidates
///    when it matched any, and [`Repair::Missing`] when it matched
///    none — an artifact the library has a record of and cannot find.
///
/// Step 1 before step 2 is what stops a byte-identical copy elsewhere
/// in the library competing for a record whose own file is fine. Step 2
/// before step 3 is what makes a swap work: two files that traded paths
/// each match the *other* record's history, so both repair in step 2
/// rather than each being read as the other's artifact changed in place,
/// and neither file is opened for extraction.
///
/// Every step reaches the library's own artifacts alone — neither the
/// item store, nor the state directory, nor a nested library can supply
/// one, and none of the three can confirm a record either. A record
/// whose last-known path lies in one of them is therefore a record this
/// library cannot find, whatever file stands there: the subtree belongs
/// to another library, or to the library's own accounting, and a
/// reconcile that confirmed itself from one would be resting on a file
/// nothing else in the library can see.
///
/// A record this run confirmed, repaired or accepted a change of is written
/// with the file's current size and modification time, so those fields
/// describe the file as borax last saw it and the next pass settles it
/// on the fast path rather than hashing it again. A record the fast
/// path settled already agrees with its file and is not written, which
/// is what keeps a routine pass over an untouched library diff-free.
///
/// No record or item is created and nothing is deleted: an orphan stays
/// an orphan — `borax adopt` is what records what is on disk — and a
/// record whose artifact is gone keeps its path and says so. Every
/// record written goes through a [`StoreWriter`], so a library with no
/// identity gains one before the run's first write, and a run that
/// writes nothing mints none; an identity file that cannot be read is
/// left as it is and reported in [`Reconciliation::identity_fault`].
///
/// Each `--accept` path that came to no accepted change — no record
/// names it, it is outside the library or excluded, its record was
/// repaired elsewhere or is ambiguous or missing, or its file belongs to
/// another record's history — is listed in
/// [`Reconciliation::not_pending`]. A named path whose record was
/// confirmed needs nothing and is not listed.
///
/// The item store is read once, and only for the gate. Never fails: a
/// record whose repair cannot be written, or whose write the identity
/// could not be minted for, is reported as [`Repair::Unwritten`] and
/// left as it was.
pub fn reconcile(root: &Path, reconciling: &Reconciling<'_>) -> Reconciliation {
    let rehash = reconciling.rehash;
    let accepted = accepted_paths(root, reconciling.accept);
    let named = |relative: &str| {
        accepted
            .iter()
            .any(|(_, named)| named.as_deref() == Some(relative))
    };
    let artifacts: Vec<String> = walk(root)
        .0
        .iter()
        .filter_map(|path| library_relative(root, path))
        .collect();
    let store = ArtifactStore::read(root);
    let records: Vec<(PathBuf, ArtifactRecord)> = store
        .files()
        .map(|(file, record)| (file.to_path_buf(), record.clone()))
        .collect();

    let mut hashes = Hashes::default();
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut outcomes: Vec<Decided> = vec![Decided::Unresolved; records.len()];

    // Step 1. Every record whose own path still holds its artifact,
    // claiming that file before step 2 looks at anything.
    for (index, (_, record)) in records.iter().enumerate() {
        // A path the library does not own is one it cannot see, so no
        // file standing there confirms anything: the record is left to
        // step 2, which reaches the library's own artifacts alone, and
        // failing that is reported as an artifact this library cannot
        // find.
        if excludes(root, &relative_to(root, &record.path)) {
            continue;
        }
        let described = describes(record, &relative_to(root, &record.path));
        if !rehash && described && !named(&record.path) {
            outcomes[index] = Decided::Confirmed { refreshed: false };
            claimed.insert(record.path.clone());
            continue;
        }
        if hashes
            .of(root, &record.path)
            .is_some_and(|hash| record.holds(&hash))
        {
            outcomes[index] = Decided::Confirmed {
                refreshed: !described,
            };
            claimed.insert(record.path.clone());
        }
    }

    // Step 2, first half: what each remaining record could be, over the
    // artifacts step 1 left unclaimed. Every record is matched before
    // any is repaired, so that two files which swapped paths are both
    // seen as candidates for each other's record.
    let mut wanted: Vec<Vec<String>> = Vec::with_capacity(records.len());
    for (index, (_, record)) in records.iter().enumerate() {
        if outcomes[index] != Decided::Unresolved {
            wanted.push(Vec::new());
            continue;
        }

        let mut current = Vec::new();
        let mut historical = Vec::new();
        for artifact in &artifacts {
            if claimed.contains(artifact) {
                continue;
            }
            let Some(hash) = hashes.of(root, artifact) else {
                continue;
            };
            if record.current_hash() == Some(&hash) {
                current.push(artifact.clone());
            } else if record.holds(&hash) {
                historical.push(artifact.clone());
            }
        }
        wanted.push(match current.is_empty() {
            true => historical,
            false => current,
        });
    }

    // Step 2, second half: repair a record with exactly one candidate
    // no other record also wants. Anything else is preserved rather
    // than guessed at, because a wrong item link is not detectable.
    for index in 0..records.len() {
        let [only] = wanted[index].as_slice() else {
            continue;
        };
        if wanted
            .iter()
            .enumerate()
            .any(|(other, candidates)| other != index && candidates.contains(only))
        {
            continue;
        }
        claimed.insert(only.clone());
        outcomes[index] = Decided::Repaired { to: only.clone() };
    }

    // Step 3. A record whose own path holds an unclaimed artifact that
    // matched nothing has changed in place: accepted on the identifier
    // extraction finds, or on the operator's word, or left pending.
    let mut items: Option<ItemStore> = None;
    for index in 0..records.len() {
        if outcomes[index] != Decided::Unresolved {
            continue;
        }
        let relative = records[index].1.path.clone();
        if claimed.contains(&relative) || !artifacts.contains(&relative) {
            continue;
        }
        let Some(hash) = hashes.of(root, &relative) else {
            continue;
        };
        if records.iter().any(|(_, other)| other.holds(&hash)) {
            continue;
        }
        let extraction = (reconciling.extract)(&relative_to(root, &relative));
        let items = items.get_or_insert_with(|| ItemStore::read(root));
        let gate = accepted_by(
            items,
            &records[index].1,
            extracted_identifier(&extraction).as_ref(),
            None,
        );
        outcomes[index] = match gate {
            Some(accepted) => Decided::Changed { hash, accepted },
            None if named(&relative) => Decided::Changed {
                hash,
                accepted: AcceptedBy::Operator,
            },
            None => Decided::Pending { hash, extraction },
        };
        claimed.insert(relative);
    }

    // Step 4, and the writing of everything the three before decided.
    let writer = StoreWriter::new(root, &store_create, &store_write);
    let mut reconciliation = Reconciliation {
        root: root.to_path_buf(),
        repairs: Vec::new(),
        records: records.len(),
        confirmed: 0,
        pending: 0,
        hashed: 0,
        not_pending: Vec::new(),
        identity_fault: None,
    };
    for (index, (file, original)) in records.iter().enumerate() {
        let mut record = original.clone();
        let (path, repair) = match &outcomes[index] {
            Decided::Confirmed { refreshed } => {
                reconciliation.confirmed += 1;
                if !refreshed {
                    continue;
                }
                refresh(root, &mut record);
                (original.path.clone(), None)
            }
            Decided::Repaired { to } => {
                record.path = to.clone();
                refresh(root, &mut record);
                let repair = Repair::Repaired {
                    from: original.path.clone(),
                };
                (to.clone(), Some(repair))
            }
            Decided::Changed { hash, accepted } => {
                // Only bytes accepted on the identifier extraction found
                // in them carry evidence: bytes accepted on the
                // operator's word were accepted as the artifact, not on
                // any identifier.
                let evidence = match accepted {
                    AcceptedBy::Extracted { identifier } => {
                        supplied(identifier)
                            .ok()
                            .map(|identifier| IdentificationEvidence {
                                identifier,
                                origin: EvidenceOrigin::Extracted,
                            })
                    }
                    AcceptedBy::Supplied { .. } | AcceptedBy::Operator => None,
                };
                let entry = HashEntry {
                    hash: hash.clone(),
                    run: reconciling.run.clone(),
                    timestamp: reconciling.timestamp.to_string(),
                    tool_version: reconciling.tool_version.to_string(),
                    item: original.item.clone(),
                    evidence,
                };
                record.append(entry, original.item.as_ref());
                refresh(root, &mut record);
                let repair = Repair::Changed {
                    hash: hash.to_string(),
                    accepted: accepted.clone(),
                };
                (original.path.clone(), Some(repair))
            }
            Decided::Pending { hash, extraction } => {
                reconciliation.pending += 1;
                let repair = Repair::Pending {
                    hash: hash.to_string(),
                    item: original.item.as_ref().map(ItemId::to_string),
                    extraction: extraction.clone(),
                };
                reconciliation
                    .repairs
                    .push((original.id.clone(), original.path.clone(), repair));
                continue;
            }
            Decided::Unresolved => {
                let candidates = wanted[index].clone();
                let repair = match candidates.is_empty() {
                    true => Repair::Missing,
                    false => Repair::Ambiguous { candidates },
                };
                reconciliation
                    .repairs
                    .push((original.id.clone(), original.path.clone(), repair));
                continue;
            }
        };

        match rewrite(file, &record, &|path, bytes| writer.write(path, bytes)) {
            Ok(()) => {
                if let Some(repair) = repair {
                    reconciliation
                        .repairs
                        .push((original.id.clone(), path, repair));
                }
            }
            Err(error) => reconciliation.repairs.push((
                original.id.clone(),
                original.path.clone(),
                Repair::Unwritten {
                    message: error.to_string(),
                },
            )),
        }
    }

    // A named path is settled when a record naming it was confirmed or
    // had its change accepted; every other one held nothing pending.
    let settled = |relative: &str| {
        records.iter().zip(&outcomes).any(|((_, record), outcome)| {
            record.path == relative
                && matches!(outcome, Decided::Confirmed { .. } | Decided::Changed { .. })
        })
    };
    reconciliation.not_pending = accepted
        .into_iter()
        .filter(|(_, named)| !named.as_deref().is_some_and(&settled))
        .map(|(given, _)| given)
        .collect();
    reconciliation.hashed = hashes.hashed;
    reconciliation.identity_fault = writer.identity_fault();
    reconciliation
}

/// The `--accept` paths `given`, each beside the library-relative path
/// it names under `root`, in the order given and with a path named
/// twice kept once.
///
/// A path is normalised as every path argument is, absolute against the
/// working directory and lexically, so two spellings of one file are one
/// path. The relative path is `None` for a path outside the library or
/// in a subtree it excludes, which names no record of it.
fn accepted_paths(root: &Path, given: &[PathBuf]) -> Vec<(PathBuf, Option<String>)> {
    let mut seen = BTreeSet::new();
    given
        .iter()
        .filter(|path| seen.insert(lexical(path).unwrap_or_else(|| path.to_path_buf())))
        .map(|path| {
            let relative = library_relative(root, path).filter(|_| !excludes(root, path));
            (path.clone(), relative)
        })
        .collect()
}

/// The events reporting `reconciliation`: one per record it has
/// something to say about, in the order it read them, then the totals.
///
/// A record the run confirmed produces no event of its own — a
/// reconcile over a library nothing has touched emits its totals and
/// nothing else — so the count of confirmations is legible only from
/// the summary, where it belongs.
pub fn reconciliation_events(reconciliation: &Reconciliation) -> Vec<Event> {
    let count = |wanted: &dyn Fn(&Repair) -> bool| {
        reconciliation
            .repairs
            .iter()
            .filter(|(_, _, repair)| wanted(repair))
            .count()
    };

    let mut events: Vec<Event> = reconciliation
        .repairs
        .iter()
        .map(|(id, path, repair)| Event::LibraryRepair {
            id: id.to_string(),
            path: path.clone(),
            repair: repair.clone(),
        })
        .collect();

    events.push(Event::LibraryReconciled {
        root: reconciliation.root.clone(),
        records: reconciliation.records,
        confirmed: reconciliation.confirmed,
        repaired: count(&|repair| matches!(repair, Repair::Repaired { .. })),
        changed: count(&|repair| matches!(repair, Repair::Changed { .. })),
        pending: count(&|repair| matches!(repair, Repair::Pending { .. })),
        ambiguous: count(&|repair| matches!(repair, Repair::Ambiguous { .. })),
        missing: count(&|repair| matches!(repair, Repair::Missing)),
        hashed: reconciliation.hashed,
    });
    events
}

/// An operator decision a library's records hold for some bytes, as
/// the content index is owed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorDecision {
    /// The artifact the record holding the decision names, as a full
    /// path.
    pub path: PathBuf,
    /// The bytes the decision is about.
    pub hash: ContentHash,
    /// The decision: evidence of origin `supplied` or `overridden`.
    pub evidence: IdentificationEvidence,
}

/// Every operator decision the artifact records of the library rooted
/// at `root` hold: one per distinct hash, with the newest operator
/// evidence a record's history holds for it
/// ([`ArtifactRecord::operator_evidence_for`]).
///
/// A hash no entry gives operator evidence for has no decision. Where
/// several records hold decisions for the same hash, the record read
/// last in path order gives it. Decisions are listed in the order their
/// hashes were first met, records in path order and each history oldest
/// first. The store is read afresh; a record file that cannot be read
/// costs its own decisions and no other. Nothing is written.
pub fn operator_decisions(root: &Path) -> Vec<OperatorDecision> {
    let mut decisions: Vec<OperatorDecision> = Vec::new();
    let mut at: BTreeMap<ContentHash, usize> = BTreeMap::new();
    for record in ArtifactStore::read(root).iter() {
        for entry in &record.history {
            let Some(evidence) = record.operator_evidence_for(&entry.hash) else {
                continue;
            };
            let decision = OperatorDecision {
                path: relative_to(root, &record.path),
                hash: entry.hash.clone(),
                evidence: evidence.clone(),
            };
            match at.get(&entry.hash) {
                Some(&index) => decisions[index] = decision,
                None => {
                    at.insert(entry.hash.clone(), decisions.len());
                    decisions.push(decision);
                }
            }
        }
    }
    decisions
}

/// The artifact record naming `path` in the library rooted at `root`,
/// or `None` when no record names it.
///
/// This is how an applying run finds the record of a file it is about
/// to move, and it asks before the move, while the path the record
/// names is still the path the file holds. Identification is by path
/// alone: a record's identity is stable across a change to its
/// artifact's bytes, so a file whose bytes no recorded hash matches is
/// still that record's artifact and not a second one.
///
/// What confirms the record — the fast path, or one of its hashes — is
/// a separate question, and [`strands`] is the only place this change
/// asks it.
///
/// `path` is a full path; a path outside the library names no record.
/// The store is read afresh, so a caller asking about several files
/// sees what each admission before it wrote.
pub fn recorded_at(root: &Path, path: &Path) -> Option<ArtifactRecord> {
    let relative = library_relative(root, path)?;
    ArtifactStore::read(root).by_path(&relative).cloned()
}

/// Whether moving the file whose hash is `hash`, with nothing written
/// to the store afterwards, would leave `record` unable to name its
/// artifact again.
///
/// `record` is what [`recorded_at`] found for the file's current path,
/// and `None` — no record names the path — strands nothing. A record
/// whose history holds `hash` survives the move: the bytes are
/// evidence a move does not touch, so reconciliation's bounded walk
/// finds the file by them wherever it lands.
///
/// A record whose history does not hold it does not survive. After the
/// move its path holds nothing, and reconciliation has no route left:
/// the walk matches by hash and finds none of its hashes anywhere,
/// and its changed-in-place step needs a file at the recorded path.
///
/// The fast path is deliberately not consulted. It compares size and
/// modification time, which a same-length in-place edit leaves alone,
/// so it can confirm a record whose every recorded hash is stale — and
/// confirmation is evidence that a record still describes a file, never
/// evidence about what the file now contains. A move is exactly what
/// takes the path away, so only content evidence counts here.
pub fn strands(record: Option<&ArtifactRecord>, hash: &ContentHash) -> bool {
    record.is_some_and(|record| !record.holds(hash))
}

/// The file name an item minted under `key` with identity `id` is
/// written with, within the library's [`ITEM_STORE`].
///
/// `<key>.<uuid>.toml`, where `<key>` is `key` folded by
/// [`borax_core::template::slug`] — so the name a person reads sorts
/// the way their bibliography sorts. `key` is the citation key the
/// `citation-keys` templates render for the item's record, given as
/// rendered; the fold is applied here so one function owns the name.
///
/// `<uuid>.toml` when `key` is `None` or folds to nothing: a name has
/// to be unique, and only the UUID makes it so.
///
/// The name is a creation-time label. Nothing renames an item file
/// afterwards, so a key that no longer matches what the templates
/// would render is not a finding; the `id` field inside the file is
/// what every reference names.
pub fn item_file_name(key: Option<&str>, id: &ItemId) -> String {
    match key.map(slug).filter(|folded| !folded.is_empty()) {
        Some(folded) => format!("{folded}.{id}.{RECORD_EXTENSION}"),
        None => format!("{id}.{RECORD_EXTENSION}"),
    }
}

/// One file's admission to a library, as an applying run settles it.
///
/// The fields travel together because no one of them answers anything
/// alone: the record decides which item the file belongs to, the hash
/// and the path decide what the artifact record says, and `held` is
/// what the library already knew about the file before the run touched
/// it.
pub struct Admitting<'a> {
    /// Where the file sits after the run's work on it — the path it
    /// was moved to, or the one it already carried. A full path, and
    /// one inside the library: a caller does not admit a file the
    /// library does not own.
    pub path: &'a Path,
    /// The record the run settled the file on. What an item minted
    /// here holds, and what the identifiers an existing item is looked
    /// up by are read from.
    pub record: &'a Record,
    /// The file's content hash, as the run computed it before the
    /// move.
    pub hash: &'a ContentHash,
    /// The artifact record naming the file before the run moved it, as
    /// [`recorded_at`] found it, and `None` when the library had none.
    pub held: Option<&'a ArtifactRecord>,
    /// Whether the operator re-identified the file in this run: they
    /// supplied the identifier it was resolved by, or accepted the
    /// record over a conflict.
    ///
    /// The one thing that moves an existing record's item link. An
    /// ordinary re-run never moves one, whatever the file resolves to.
    pub reidentified: bool,
    /// The evidence the record identifies the file's bytes by, written
    /// on any history entry the admission appends, and `None` when the
    /// record was reached by no identifier the admission is to record.
    ///
    /// Evidence is part of the decision: re-identifying the newest
    /// bytes with evidence their newest entry does not carry appends an
    /// entry even when the item they answer with is unchanged.
    pub evidence: Option<&'a IdentificationEvidence>,
    /// The citation key an item minted here is named by, as the
    /// templates rendered it and before [`item_file_name`] folds it.
    pub key: Option<&'a str>,
    /// The run a hash entry written here is stamped with.
    pub run: RunId,
    /// The timestamp a hash entry written here is stamped with.
    pub timestamp: &'a str,
    /// The borax version a hash entry written here is stamped with.
    pub tool_version: &'a str,
}

/// What an admission wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    /// The artifact record's identity: the one the library already
    /// held for the file, or the one minted for it.
    pub artifact: ArtifactId,
    /// The item the artifact record names afterwards.
    pub item: ItemId,
    /// The item it named before, when the admission moved the link,
    /// and `None` when the link is where it was or the record is new.
    pub relinked_from: Option<ItemId>,
}

/// A store write that did not land, with `message` as the filesystem
/// put it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unrecorded {
    pub message: String,
}

/// Record the admission of one file to the library rooted at `root`.
///
/// Writes the item first and the artifact record second, through
/// `write`. Nothing makes the two writes one transaction — the writers
/// are peers and no lock may be a precondition — so the order is
/// chosen for what an interruption leaves: an item nothing links to,
/// which is an ordinary library state `borax validate` counts, rather
/// than an artifact record naming an item that does not exist, which
/// is a dangling link and a finding.
///
/// **Which item.** A record the library already holds for the file
/// keeps the item it names, whatever the file resolved to this time:
/// selection happens when a record is minted, so an ordinary re-run
/// cannot move an artifact to another item and cannot accumulate a
/// fresh item behind a file carrying no identifier. The two exceptions
/// select as a mint does: a re-identification
/// ([`Admitting::reidentified`]), which is the operator correcting
/// what the file is, and a held record naming no item at all.
///
/// Selecting means the item the library already holds carrying one of
/// the resolved record's identifiers — searched DOI, arXiv, PMID,
/// ISBN, over the item store in read order, first match winning — or
/// an item minted from that record when the library holds none. A
/// record with no identifier at all still gets an item, since an
/// item's identity is its minted UUID and identifiers are optional: two
/// different files carrying no identifier become two items, which
/// states what borax knows rather than guessing a match.
///
/// An item the library holds with no artifact recorded against it is
/// selected like any other. Such an item is the case the model exists
/// for — a work cited before its PDF was had — so the file that
/// finally supplies it is its first artifact and no second item is
/// minted.
///
/// A minted item is written at [`ITEM_STORE`]`/`[`item_file_name`];
/// an item that was already there is not rewritten, an admission
/// having nothing to add to it.
///
/// **What the artifact record says.** Its identity is the held
/// record's, or one minted for a file that had none. Its path is
/// `path` rendered library-relative, and its size and modification
/// time are the file's as it stands now, so the next reconcile settles
/// it on the fast path and hashes nothing.
///
/// **Its history.** A minted record's one entry names the item it
/// links. A held record whose history does not hold the file's hash
/// has it appended, naming the item the record links afterwards: the
/// caller passes such a record only for bytes it has accepted as that
/// artifact, and passes no held record for bytes it separates from it.
/// Bytes the history already holds are appended again only when the
/// admission decides which item they belong to — a re-identification,
/// or a record that linked no item gaining one — and they are not
/// already the newest entry answering with that item; an ordinary
/// re-run over restored bytes appends nothing, so they keep answering
/// with the item they were accepted under. Before an entry for other
/// bytes, [`ArtifactRecord::append`] restates the newest bytes under
/// the link as it was read, where that link was corrected by hand.
///
/// A minted record is written at
/// [`STATE_DIR`]`/`[`ARTIFACT_STORE`]`/<artifact-uuid>.toml`, flat and
/// named for the identity inside it; a record the library already held
/// is written back to the file it was read from, whatever that file is
/// called.
///
/// A record the library already held is edited as a document through
/// `toml_edit`, so its key order, its formatting and any comment a
/// person left in it survive; a minted one is written whole. Either
/// way the bytes are handed to `write`, which is
/// [`write_atomically`] in a run and a failing stand-in in a test.
///
/// **Failure.** Fails with [`Unrecorded`] on the first write that does
/// not land, and on a file that cannot be stat'd — a record with no
/// size and no modification time would be one reconciliation's fast
/// path can never settle. A held record no longer in the store, with
/// the identity and the path it was read with, fails too: its file is
/// gone, and writing one back would restore a record someone removed.
/// An item write that fails writes no artifact
/// record. An artifact write that fails leaves the item, which is the
/// residue the order was chosen for. Nothing is retried: the caller
/// reports the failure and the next applying run over the file records
/// it again.
pub fn admit(
    root: &Path,
    admitting: &Admitting<'_>,
    write: &dyn Fn(&Path, &[u8]) -> io::Result<()>,
) -> Result<Admitted, Unrecorded> {
    let unrecorded = |message: String| Unrecorded { message };
    let relative = library_relative(root, admitting.path).ok_or_else(|| {
        unrecorded(format!(
            "{} is not inside the library at {}",
            admitting.path.display(),
            root.display()
        ))
    })?;
    let (size, modified_millis) = seen(admitting.path).ok_or_else(|| {
        unrecorded(format!(
            "cannot read the size and modification time of {}",
            admitting.path.display()
        ))
    })?;

    let kept = admitting
        .held
        .and_then(|held| held.item.clone())
        .filter(|_| !admitting.reidentified);
    let item = match kept {
        Some(item) => item,
        None => {
            select_item(root, admitting, write).map_err(|error| unrecorded(error.to_string()))?
        }
    };

    let entry = HashEntry {
        hash: admitting.hash.clone(),
        run: admitting.run.clone(),
        timestamp: admitting.timestamp.to_string(),
        tool_version: admitting.tool_version.to_string(),
        item: Some(item.clone()),
        evidence: admitting.evidence.cloned(),
    };

    let Some(held) = admitting.held else {
        let record = ArtifactRecord {
            id: ArtifactId::from_uuid(Uuid::now_v7()),
            item: Some(item.clone()),
            path: relative,
            size,
            modified_millis,
            history: vec![entry],
        };
        let file = root
            .join(STATE_DIR)
            .join(ARTIFACT_STORE)
            .join(format!("{}.{RECORD_EXTENSION}", record.id));
        write_whole(&file, &record.to_toml(), write)
            .map_err(|error| unrecorded(error.to_string()))?;
        return Ok(Admitted {
            artifact: record.id,
            item,
            relinked_from: None,
        });
    };

    let store = ArtifactStore::read(root);
    let file = store
        .files()
        .find(|(_, record)| record.id == held.id && record.path == held.path)
        .map(|(file, _)| file.to_path_buf())
        .ok_or_else(|| {
            unrecorded(format!(
                "artifact record {} is no longer in the store",
                held.id
            ))
        })?;

    let mut record = held.clone();
    record.item = Some(item.clone());
    record.path = relative;
    record.size = size;
    record.modified_millis = modified_millis;
    settle_history(&mut record, admitting, held.item.as_ref());
    rewrite(&file, &record, write).map_err(|error| unrecorded(error.to_string()))?;

    Ok(Admitted {
        artifact: record.id,
        relinked_from: held.item.clone().filter(|before| before != &item),
        item,
    })
}

/// The identifiers of `record` an item is looked up by, in the order
/// they are searched: DOI, arXiv, PMID, ISBN. Empty for a record
/// carrying none.
fn identifiers(record: &Record) -> Vec<Identifier> {
    [
        record.doi.clone().map(Identifier::Doi),
        record.borax.arxiv.clone().map(Identifier::Arxiv),
        record.pmid.map(Identifier::Pmid),
        record.isbn.clone().map(Identifier::Isbn),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The item an admission of `admitting` links to when it selects one:
/// the first item of the library rooted at `root` carrying one of the
/// record's [`identifiers`], or an item minted from the record and
/// written through `write` when the library holds none.
///
/// A minted item is written whole at [`ITEM_STORE`]`/`
/// [`item_file_name`]. Fails with what `write` said, in which case no
/// item carries the identity minted for it.
fn select_item(
    root: &Path,
    admitting: &Admitting<'_>,
    write: &dyn Fn(&Path, &[u8]) -> io::Result<()>,
) -> io::Result<ItemId> {
    let items = ItemStore::read(root);
    if let Some(found) = identifiers(admitting.record)
        .iter()
        .find_map(|identifier| items.by_identifier(identifier))
    {
        return Ok(found.id.clone());
    }

    let item = Item {
        id: ItemId::from_uuid(Uuid::now_v7()),
        record: admitting.record.clone(),
    };
    let file = root
        .join(ITEM_STORE)
        .join(item_file_name(admitting.key, &item.id));
    write_whole(&file, &item.to_toml(), write)?;
    Ok(item.id)
}

/// Write `text` to `path` through `write`, as a whole new file.
///
/// Fails without writing when `text` is empty, which is what a record's
/// rendering yields for a value TOML cannot hold: an empty file would
/// be one no reader takes for a record.
fn write_whole(
    path: &Path,
    text: &str,
    write: &dyn Fn(&Path, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    if text.is_empty() {
        return Err(io::Error::other(format!(
            "{} could not be rendered as TOML",
            path.display()
        )));
    }
    write(path, text.as_bytes())
}

/// Write `bytes` to `path` as a store write: atomically, creating the
/// store's directory if this is the library's first record.
///
/// The `write` seam of a run's [`StoreWriter`], through which [`admit`]
/// and [`reconcile`] write. A reader sees either the file as it was or
/// the file as it now is, and an interrupted write leaves the previous
/// one intact with no temporary a reader would take for a record. Mints
/// no identity: that is the writer's.
pub fn store_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomically(path, bytes)
}

/// The event reporting what an admission of the file now at `path`
/// came to, or `None` when it came to nothing worth reporting.
///
/// An admission that minted or updated a record and left its link
/// where it was says nothing: the file's own outcome event already
/// reports that borax settled it.
pub fn admission_event(path: &Path, admitted: &Result<Admitted, Unrecorded>) -> Option<Event> {
    let admission = match admitted {
        Ok(Admitted {
            artifact,
            item,
            relinked_from: Some(from),
        }) => Admission::Relinked {
            id: artifact.to_string(),
            from: from.to_string(),
            to: item.to_string(),
        },
        Ok(_) => return None,
        Err(unrecorded) => Admission::Unwritten {
            message: unrecorded.message.clone(),
        },
    };
    Some(Event::LibraryAdmission {
        path: path.to_path_buf(),
        admission,
    })
}

/// What an adoption made of a library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adoptions {
    /// The library adopted into, as [`Survey::root`].
    pub root: PathBuf,
    /// Every orphan the walk found, library-relative, each exactly once
    /// with what became of it, in the order the walk found them.
    pub adoptions: Vec<(String, Adoption)>,
    /// How many orphans the library held before the run.
    pub orphans: usize,
    /// The library's identity file as the run's first write found it
    /// unreadable, when it did: the fault
    /// [`identity_fault_warning`] reports. `None` for a run that
    /// recorded nothing, whatever the file holds.
    pub identity_fault: Option<StoreFault>,
}

impl Adoptions {
    /// How many orphans the run gave an artifact record.
    pub fn adopted(&self) -> usize {
        self.adoptions
            .iter()
            .filter(|(_, adoption)| matches!(adoption, Adoption::Recorded { .. }))
            .count()
    }
}

/// The record an adoption lookup found for an orphan's bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Adopted {
    /// The record found.
    pub record: Record,
    /// The identifier the record was found by: the one the content
    /// index answers with for the bytes.
    pub identifier: Identifier,
    /// The service whose stored answer the record is.
    pub service: SourceName,
}

/// Why an adoption lookup found no record for an orphan's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unadopted {
    /// The content index answers with no evidence for the bytes.
    Unindexed,
    /// The content index answers with `identifier`, and no stored
    /// answer holds a record for it.
    Unanswered { identifier: Identifier },
}

/// Adopt the orphans of the library rooted at `root`: record each one
/// whose bytes `lookup` finds a record for.
///
/// Each orphan ([`orphans`]) is hashed and its hash handed to
/// `lookup`, which in a run reads the content index's evidence and the
/// response cache. Where it finds a record, the orphan is admitted
/// exactly as an applying run admits a file it found already named
/// ([`admit`], with no held record and no re-identification), and its
/// history entry carries the identifier the record was found by, with
/// origin `adopted`: the item the library holds for one of the
/// record's identifiers is linked, or one is minted from the record and
/// written at the name `key` renders for it. The store is re-read on
/// every admission, so a second orphan of the work a first one minted
/// an item for links to that item. [`Adoption::Recorded`] names the
/// service's response cache as the record's retrieval. Where `lookup`
/// finds none, the orphan is left as it is and reported
/// [`Adoption::Unindexed`] or [`Adoption::Unanswered`], as `lookup`
/// says.
///
/// An orphan whose hash is in the history of a record already in the
/// store, or of one this run wrote for an earlier orphan, is left an
/// orphan and reported [`Adoption::Held`], and `lookup` is not asked
/// about it.
///
/// Nothing but the two stores and the library's identity is written:
/// the identity is minted before the first record when the library has
/// none ([`StoreWriter`]), no file is opened beyond being hashed, and
/// none is moved, renamed or deleted. A record that
/// already names a path is not an orphan's, so it is left exactly as it
/// is, and a second run over a library the first left alone finds
/// nothing to write.
///
/// Every orphan gets exactly one adoption, so the orphans the run
/// leaves are the adoptions other than [`Adoption::Recorded`].
///
/// `run`, `timestamp` and `tool_version` stamp the one hash entry each
/// new record carries. Never fails: an orphan that cannot be read is
/// reported [`Adoption::Unreadable`], and one whose record cannot be
/// written [`Adoption::Unwritten`], each still an orphan afterwards.
pub fn adopt(
    root: &Path,
    lookup: &dyn Fn(&ContentHash) -> Result<Adopted, Unadopted>,
    key: &mut dyn FnMut(&Record, &ContentHash) -> Option<String>,
    run: RunId,
    timestamp: &str,
    tool_version: &str,
) -> Adoptions {
    let store = ArtifactStore::read(root);
    let orphans = orphans(root, &walk(root).0, &store);
    let mut written: BTreeMap<ContentHash, ArtifactId> = BTreeMap::new();
    let mut adoptions = Vec::new();
    let writer = StoreWriter::new(root, &store_create, &store_write);

    for path in &orphans {
        let Some(relative) = library_relative(root, path) else {
            continue;
        };
        let hash = match hash_file(path) {
            Ok(hash) => hash,
            Err(error) => {
                let message = error.to_string();
                adoptions.push((relative, Adoption::Unreadable { message }));
                continue;
            }
        };
        let holder = store
            .by_hash(&hash)
            .first()
            .map(|record| record.id.clone())
            .or_else(|| written.get(&hash).cloned());
        if let Some(id) = holder {
            let id = id.to_string();
            adoptions.push((relative, Adoption::Held { id }));
            continue;
        }
        let found = match lookup(&hash) {
            Ok(found) => found,
            Err(Unadopted::Unindexed) => {
                adoptions.push((relative, Adoption::Unindexed));
                continue;
            }
            Err(Unadopted::Unanswered { identifier }) => {
                let identifier = identifier.to_string();
                adoptions.push((relative, Adoption::Unanswered { identifier }));
                continue;
            }
        };
        let record = found.record;
        let evidence = IdentificationEvidence {
            identifier: found.identifier,
            origin: EvidenceOrigin::Adopted,
        };

        let key = key(&record, &hash);
        let admitted = admit(
            root,
            &Admitting {
                path,
                record: &record,
                hash: &hash,
                held: None,
                reidentified: false,
                evidence: Some(&evidence),
                key: key.as_deref(),
                run: run.clone(),
                timestamp,
                tool_version,
            },
            &|path, bytes| writer.write(path, bytes),
        );
        let adoption = match admitted {
            Ok(admitted) => {
                written.insert(hash, admitted.artifact.clone());
                Adoption::Recorded {
                    id: admitted.artifact.to_string(),
                    item: admitted.item.to_string(),
                    retrieval: RetrievedFrom::ServiceCache {
                        service: found.service.as_str().to_string(),
                    },
                }
            }
            Err(unrecorded) => Adoption::Unwritten {
                message: unrecorded.message,
            },
        };
        adoptions.push((relative, adoption));
    }

    Adoptions {
        root: root.to_path_buf(),
        adoptions,
        orphans: orphans.len(),
        identity_fault: writer.identity_fault(),
    }
}

/// The events reporting `adoptions`, apart: one per orphan, in the
/// order it reached them, and the totals.
///
/// Apart so that a caller can report what is about the run as a whole
/// between the two, and still close with the totals.
pub fn adoption_events(adoptions: &Adoptions) -> (Vec<Event>, Event) {
    let events = adoptions
        .adoptions
        .iter()
        .map(|(path, adoption)| Event::LibraryAdoption {
            path: path.clone(),
            adoption: adoption.clone(),
        })
        .collect();
    let adopted = adoptions.adopted();
    let totals = Event::LibraryAdopted {
        root: adoptions.root.clone(),
        adopted,
        orphans: adoptions.orphans - adopted,
    };
    (events, totals)
}
