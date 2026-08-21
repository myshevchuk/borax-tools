//! The collection's ledger on disk: where it lives, how it is read,
//! and what a run does about what it says.
//!
//! [`borax_core::ledger`] holds the values — entries, the index, the
//! duplicate verdicts — and knows nothing about files. This module is
//! the adapter around it: the JSON Lines file under the collection's
//! `.borax/`, the scan that regenerates it from the files themselves,
//! and the translation of a load's warnings into something the run
//! says out loud.
//!
//! Everything here treats the ledger as derived accounting. It is
//! rebuildable from the collection, so nothing it says is allowed to
//! stop a run: a ledger that is missing, corrupt, or disabled turns
//! duplicate detection off and warns, and a duplicate whose recorded
//! file is no longer on disk is not a duplicate at all.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use borax_core::bib_output::parse_sidecar_record;
use borax_core::content::ContentHash;
use borax_core::ledger::{Duplicate, Entry, Index, RunId, Unparsable, Warning, parse_jsonl};
use borax_core::record::Record;
use borax_sources::store::hash_file;

use crate::bib::sidecar_path;
use crate::event::{Diagnostic, Level};
use crate::pipeline::FileRecord;
use crate::run::inputs;

/// The directory a collection keeps its accounting in, directly under
/// the collection root.
pub const ACCOUNTING_DIR: &str = ".borax";

/// The ledger's name within that directory.
pub const LEDGER_FILE: &str = "ledger.jsonl";

/// The ledger of a collection, as something a run can read and add to.
///
/// The one seam to the filesystem here, so the pipeline's duplicate
/// checks and the warnings they produce are testable without a
/// collection on disk.
pub trait Ledger {
    /// Read the whole ledger.
    ///
    /// Never fails: everything that can go wrong with reading it is a
    /// [`LedgerWarning`] the caller reports and carries on from.
    fn load(&self) -> Loaded;

    /// Append `entries` in the order given.
    ///
    /// The whole slice lands or none of it does, so a reader between
    /// two appends never sees half a run.
    fn append(&self, entries: &[Entry]) -> io::Result<()>;
}

/// What reading a ledger revealed beyond its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerWarning {
    /// There is no ledger to read.
    Absent,
    /// A line the writer had finished does not parse, so the file's
    /// contents cannot be trusted as a whole.
    Unparsable(Unparsable),
    /// The final line was cut off mid-append. Everything before it
    /// stands.
    TornTrailingLine,
}

/// A ledger as read, and what reading it revealed.
///
/// `index` holds only entries worth trusting: an absent ledger and one
/// with a finished line that does not parse both index nothing, since
/// neither says reliably what the collection holds. A torn trailing
/// line costs its own entry alone, so `index` carries everything the
/// parse recovered before it.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub index: Index,
    pub warning: Option<LedgerWarning>,
}

/// A [`Ledger`] backed by a JSON Lines file.
#[derive(Debug, Clone)]
pub struct FileLedger {
    path: PathBuf,
}

impl FileLedger {
    /// A ledger stored at `path`. The file and its parent directory are
    /// created on the first append.
    pub fn new(path: impl Into<PathBuf>) -> FileLedger {
        FileLedger { path: path.into() }
    }

    /// The ledger of the collection rooted at `root`, at
    /// [`LEDGER_FILE`] under its [`ACCOUNTING_DIR`].
    ///
    /// The accounting sits inside the collection, so a collection
    /// moved, re-mounted, or opened on another machine keeps it.
    pub fn at_collection_root(root: impl AsRef<Path>) -> FileLedger {
        FileLedger::new(root.as_ref().join(ACCOUNTING_DIR).join(LEDGER_FILE))
    }

    /// The file entries are read from and appended to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Ledger for FileLedger {
    /// A file that cannot be read at all — absent, or unreadable —
    /// reports [`LedgerWarning::Absent`]: to a run, a ledger it cannot
    /// see and one that was never written are the same situation.
    fn load(&self) -> Loaded {
        let empty = |warning| Loaded {
            index: Index::build(&[]),
            warning: Some(warning),
        };

        let Ok(contents) = fs::read_to_string(&self.path) else {
            return empty(LedgerWarning::Absent);
        };

        match parse_jsonl(&contents) {
            Err(unparsable) => empty(LedgerWarning::Unparsable(unparsable)),
            Ok(parsed) => Loaded {
                index: Index::build(&parsed.entries),
                warning: parsed
                    .warning
                    .map(|Warning::TornTrailingLine| LedgerWarning::TornTrailingLine),
            },
        }
    }

    /// Appends the entries as one write, each as a line of JSON, in the
    /// order given: the file is a log of admissions as they happened,
    /// and only `borax ledger rebuild` sorts it.
    ///
    /// Lines that cannot be serialized are dropped rather than written
    /// malformed, since a finished line that does not parse costs the
    /// whole file its credibility and not just its own entry.
    fn append(&self, entries: &[Entry]) -> io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut batch = String::new();
        for entry in entries {
            if let Ok(line) = serde_json::to_string(entry) {
                batch.push_str(&line);
                batch.push('\n');
            }
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(batch.as_bytes())
    }
}

/// The duplicate detection a run works with, and what it has to say
/// about it.
///
/// `index` is empty whenever detection is off, so a caller checks
/// against it without asking whether there was a ledger at all.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub index: Index,
    pub diagnostic: Option<Diagnostic>,
}

/// Get the run's duplicate detection ready.
///
/// `enabled` is the `--no-ledger` gate and `ledger` is `None` when the
/// run is outside any collection. Either way nothing is read and
/// nothing is said: a run that was told not to keep accounting, or has
/// nowhere to keep it, has no cause to complain about not finding any.
///
/// Otherwise the ledger is loaded and any [`LedgerWarning`] becomes a
/// single warning [`Diagnostic`] naming the cause. The run continues in
/// every case — the ledger can report a duplicate, never veto a run.
pub fn prepare(enabled: bool, ledger: Option<&dyn Ledger>) -> Prepared {
    let off = || Prepared {
        index: Index::build(&[]),
        diagnostic: None,
    };

    if !enabled {
        return off();
    }
    let Some(ledger) = ledger else {
        return off();
    };

    let Loaded { index, warning } = ledger.load();
    Prepared {
        index,
        diagnostic: warning.map(|warning| Diagnostic {
            level: Level::Warning,
            message: message_for(&warning),
        }),
    }
}

/// What a run says about `warning`.
fn message_for(warning: &LedgerWarning) -> String {
    match warning {
        LedgerWarning::Absent => {
            "this collection has no ledger yet, so duplicate detection is off for this run"
                .to_string()
        }
        LedgerWarning::Unparsable(unparsable) => format!(
            "the ledger is unreadable ({unparsable}), so duplicate detection is off for this run; \
             borax ledger rebuild restores it"
        ),
        LedgerWarning::TornTrailingLine => {
            "the ledger's last line was cut off mid-append and was ignored; borax ledger rebuild \
             restores it"
                .to_string()
        }
    }
}

/// The full path of `relative`, which is `/`-separated and relative to
/// `collection_root` as a ledger entry's path is.
///
/// The separator is the ledger's own rather than the platform's, so an
/// entry written on one machine names the same file on another.
pub fn relative_to(collection_root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(collection_root.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

/// Whether the file `duplicate` names is still in the collection.
///
/// `exists` answers whether a path is there, and disk is what decides:
/// an entry whose file was undone, deleted, or moved away records an
/// admission that no longer holds, and a false duplicate report would
/// keep a file out of a collection that does not have it.
pub fn duplicate_is_live(
    duplicate: &Duplicate,
    collection_root: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> bool {
    exists(&relative_to(collection_root, &duplicate.existing_path))
}

/// A collection to check an incoming file against.
///
/// The three travel together because no one of them answers anything
/// alone: a match in `ledger` names a collection-relative path, `root`
/// is what makes it a path on this machine, and `exists` is what says
/// whether the file is still there.
pub struct Collection<'a> {
    /// The ledger's entries, keyed for lookup. Empty when duplicate
    /// detection is off, which makes every check miss.
    pub ledger: &'a Index,
    /// The directory the ledger's paths are relative to.
    pub root: &'a Path,
    /// Whether a path is in the collection, answering as
    /// [`Path::exists`] does.
    pub exists: &'a dyn Fn(&Path) -> bool,
}

impl Collection<'_> {
    /// The full path of `duplicate`, or `None` when the file it names
    /// is no longer there.
    pub(crate) fn live_path(&self, duplicate: &Duplicate) -> Option<PathBuf> {
        match duplicate_is_live(duplicate, self.root, self.exists) {
            true => Some(relative_to(self.root, &duplicate.existing_path)),
            false => None,
        }
    }
}

/// One file found in a collection, with everything a ledger entry
/// needs from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Scanned {
    /// Where the file sits, relative to the collection root and
    /// `/`-separated.
    pub path: String,
    pub hash: ContentHash,
    /// The record recovered from the file's sidecar.
    pub record: Record,
}

/// The ledger entries a scanned collection amounts to.
///
/// One entry per scanned file, each stamped with `run`, `timestamp`,
/// and `tool_version` — the whole rebuild is one admission, so all
/// three are the same across the result. Entries for files the scan did
/// not find are simply not produced, which is what compacts a rebuild.
///
/// The order is the scan's; sorting is
/// [`borax_core::ledger::serialize_jsonl`]'s, which is what makes two
/// rebuilds of an unchanged collection byte identical.
pub fn rebuild(scanned: &[Scanned], run: RunId, timestamp: &str, tool_version: &str) -> Vec<Entry> {
    scanned
        .iter()
        .map(|scanned| {
            entry_for(
                &scanned.record,
                scanned.hash.clone(),
                &scanned.path,
                run.clone(),
                timestamp,
                tool_version,
            )
        })
        .collect()
}

/// Every file under `root` a ledger entry can be made of.
///
/// A file counts when it is a PDF and its sidecar yields a record: the
/// sidecar is where the identifiers are, and an entry without them
/// could only ever match by content. A file with no sidecar, one whose
/// sidecar holds no record borax wrote, and one that cannot be hashed
/// each contribute nothing rather than an entry that says less than it
/// claims.
///
/// Order is [`crate::run::inputs`]'s, which sorts a directory's files
/// by path.
pub fn scan_collection(root: &Path) -> Vec<Scanned> {
    inputs(&[root.to_path_buf()])
        .into_iter()
        .filter_map(|path| {
            Some(Scanned {
                path: collection_relative(root, &path)?,
                record: parse_sidecar_record(&fs::read_to_string(sidecar_path(&path)).ok()?)?,
                hash: hash_file(&path).ok()?,
            })
        })
        .collect()
}

/// `path` as a ledger entry's path: relative to `root` and
/// `/`-separated whatever the platform writes.
///
/// `None` when `path` is not below `root`, which no ledger entry can
/// describe.
fn collection_relative(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let segments: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(segments.join("/"))
}

/// The ledger entry an applied admission of `file` to `path` appends.
///
/// `path` is relative to the collection root and `/`-separated.
/// `None` when the file's hash is unknown: an entry no later run could
/// match by content records an admission the ledger cannot recognise
/// again.
pub fn admission_entry(
    file: &FileRecord,
    path: &str,
    run: RunId,
    timestamp: &str,
    tool_version: &str,
) -> Option<Entry> {
    Some(entry_for(
        &file.record,
        file.hash.clone()?,
        path,
        run,
        timestamp,
        tool_version,
    ))
}

/// The entry recording `record` at `path`, carrying every identifier
/// the record holds.
fn entry_for(
    record: &Record,
    hash: ContentHash,
    path: &str,
    run: RunId,
    timestamp: &str,
    tool_version: &str,
) -> Entry {
    Entry {
        hash,
        doi: record.doi.clone(),
        arxiv: record.borax.arxiv.clone(),
        pmid: record.pmid,
        isbn: record.isbn.clone(),
        path: path.to_string(),
        entry_type: record.entry_type,
        run,
        timestamp: timestamp.to_string(),
        tool_version: tool_version.to_string(),
    }
}
