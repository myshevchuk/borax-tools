//! Bibliography output: the master `.bib` file and the per-file
//! sidecars.
//!
//! This is where a resolved record stops being an internal value and
//! becomes something a LaTeX document cites. Two destinations, written
//! independently so that a failure at one does not cost the other:
//! the master file accumulates every entry a library has ever produced,
//! and a sidecar carries one file's entry beside the file itself, which
//! is what survives the library being reorganised by something that is
//! not borax.
//!
//! The merge rules — dedup by identifier, key uniqueness, byte
//! preservation of untouched content — belong to
//! [`borax_core::bib_output`]. What lives here is which files are
//! written, under which key, and what the run says about it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use borax_core::bib_output::{DuplicatePolicy, MergeOutcome, merge, sidecar};
use borax_core::content::ContentHash;
use borax_core::record::Record;
use borax_core::template::{RenderInput, TemplateTable};

use crate::event::{Event, SkipReason};
use crate::pipeline::FileRecord;

/// The extension a sidecar carries.
pub const SIDECAR_EXTENSION: &str = "bib";

/// Where bibliography output goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibConfig {
    /// The master `.bib` every entry is merged into, or `None` when the
    /// run keeps no master file.
    pub path: Option<PathBuf>,
    /// What to do when an entry for the same work is already there.
    pub duplicates: DuplicatePolicy,
    /// Whether to write a sidecar beside each resolved file.
    pub sidecars: bool,
}

/// The file operations bibliography output needs.
///
/// The one seam to the filesystem in this module, so the merge and the
/// event stream are testable without touching a disk.
pub trait BibFiles {
    /// The current content of `path`.
    ///
    /// A path that does not exist reads as the empty string: a master
    /// file is created by the first entry merged into it, and its
    /// absence is not a failure.
    fn read(&self, path: &Path) -> io::Result<String>;

    /// Replace the content of `path`, creating it and its parent
    /// directories if they are absent.
    fn write(&self, path: &Path, content: &str) -> io::Result<()>;
}

/// Characters a citation key cannot carry: they end the key, open or
/// close a group, or start a comment where BibTeX reads one.
const FORBIDDEN_IN_KEY: [char; 4] = [',', '{', '}', '%'];

/// The citation key `record` is cited under.
///
/// Rendered from `templates` exactly as a filename is, then stripped of
/// the characters a BibTeX key cannot carry — whitespace and
/// [`FORBIDDEN_IN_KEY`] — so a work is cited under the same name its
/// file is stored under. `hash` supplies the `sha1` field a template
/// may use.
///
/// Returns `None` when the template renders nothing, or nothing that
/// survives the stripping — a record too sparse to cite.
pub fn citation_key(
    record: &Record,
    hash: Option<&ContentHash>,
    templates: &TemplateTable,
) -> Option<String> {
    let key: String = templates
        .render(&RenderInput {
            record,
            sha1: hash.map(ContentHash::as_str),
        })
        .chars()
        .filter(|character| !character.is_whitespace() && !FORBIDDEN_IN_KEY.contains(character))
        .collect();

    match key.is_empty() {
        true => None,
        false => Some(key),
    }
}

/// The sidecar path for the file at `path`: the same path with its
/// extension replaced by [`SIDECAR_EXTENSION`].
pub fn sidecar_path(path: &Path) -> PathBuf {
    path.with_extension(SIDECAR_EXTENSION)
}

/// Write bibliography output for `resolved`.
///
/// Per file, in the order given:
///
/// 1. A citation key is rendered ([`citation_key`]). A record that
///    renders none is [`crate::event::SkipReason::Unciteable`] and
///    contributes nothing to either destination.
/// 2. When [`BibConfig::sidecars`] is set, a sidecar
///    ([`borax_core::bib_output::sidecar`]) is written beside the file,
///    reported as [`Event::Sidecar`].
///
/// Then, when [`BibConfig::path`] names a master file, every keyed
/// record is merged into it in one pass
/// ([`borax_core::bib_output::merge`]) and the file rewritten once.
/// Merging as one batch is what lets the merge assign unique keys
/// across the whole run rather than one file at a time. Each addition
/// reports an [`Event::BibEntry`] naming the key it landed under and
/// the outcome — `added`, `already-present`, or `updated`.
///
/// A read or write that fails is
/// [`crate::event::SkipReason::BibWriteFailed`] for the files it cost,
/// and the run continues: bibliography output is the last thing a run
/// does, and losing it does not undo the renames that preceded it.
///
/// Events come back in a fixed order — every sidecar event in input
/// order, then every master-file event in input order — so two runs
/// over the same inputs produce the same stream.
pub fn write_bib(
    resolved: &[(PathBuf, FileRecord)],
    templates: &TemplateTable,
    config: &BibConfig,
    files: &dyn BibFiles,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut keyed = Vec::new();

    for (path, file) in resolved {
        let Some(key) = citation_key(&file.record, file.hash.as_ref(), templates) else {
            events.push(Event::Skipped {
                path: path.clone(),
                reason: SkipReason::Unciteable,
            });
            continue;
        };

        if config.sidecars {
            let target = sidecar_path(path);
            events.push(match files.write(&target, &sidecar(&file.record, &key)) {
                Ok(()) => Event::Sidecar {
                    path: path.clone(),
                    target,
                },
                // The two destinations are independent, so a file whose
                // sidecar failed still goes to the master file.
                Err(error) => {
                    bib_failed(path, format!("writing \"{}\": {error}", target.display()))
                }
            });
        }

        keyed.push(Keyed {
            path,
            record: &file.record,
            key,
        });
    }

    let Some(master) = &config.path else {
        return events;
    };
    // A run with nothing to merge leaves the master file as it is,
    // rather than reading and rewriting it byte for byte.
    if keyed.is_empty() {
        return events;
    }

    let existing = match files.read(master) {
        Ok(existing) => existing,
        Err(error) => {
            events.extend(keyed.iter().map(|entry| {
                bib_failed(
                    entry.path,
                    format!("reading \"{}\": {error}", master.display()),
                )
            }));
            return events;
        }
    };

    let additions: Vec<(&str, &Record)> = keyed
        .iter()
        .map(|entry| (entry.key.as_str(), entry.record))
        .collect();
    let merged = merge(&existing, &additions, config.duplicates);

    events.extend(match files.write(master, &merged.content) {
        Ok(()) => keyed
            .iter()
            .zip(&merged.outcomes)
            .map(|(entry, outcome)| entry_event(entry.path, outcome))
            .collect::<Vec<Event>>(),
        Err(error) => keyed
            .iter()
            .map(|entry| {
                bib_failed(
                    entry.path,
                    format!("writing \"{}\": {error}", master.display()),
                )
            })
            .collect(),
    });
    events
}

/// One resolved file that has a citation key, and so a place in both
/// destinations.
struct Keyed<'a> {
    path: &'a Path,
    record: &'a Record,
    key: String,
}

/// The [`Event::BibEntry`] reporting what the master-file merge did with
/// one addition.
///
/// The key named is the one the entry ends up cited under: the requested
/// key as the merge issued it for an addition or an update, and the
/// pre-existing entry's key for a duplicate the merge left alone.
fn entry_event(path: &Path, outcome: &MergeOutcome) -> Event {
    let (key, name) = match outcome {
        MergeOutcome::Added { key } => (key, "added"),
        MergeOutcome::AlreadyPresent { existing_key } => (existing_key, "already-present"),
        MergeOutcome::Updated { key } => (key, "updated"),
    };
    Event::BibEntry {
        path: path.to_path_buf(),
        key: key.clone(),
        outcome: name.to_string(),
    }
}

/// [`SkipReason::BibWriteFailed`] for `path`, carrying `message`.
///
/// `message` names the file that could not be read or written: a
/// record's bibliography output has two destinations, and the message is
/// what tells a reader which of them was lost.
fn bib_failed(path: &Path, message: String) -> Event {
    Event::Skipped {
        path: path.to_path_buf(),
        reason: SkipReason::BibWriteFailed { message },
    }
}

/// A [`BibFiles`] backed by the real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealBibFiles;

impl BibFiles for RealBibFiles {
    fn read(&self, path: &Path) -> io::Result<String> {
        match fs::read_to_string(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            result => result,
        }
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)
    }
}
