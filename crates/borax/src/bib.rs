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

use borax_core::bib_output::{DuplicatePolicy, MergeOutcome, merge, parse_sidecar_record, sidecar};
use borax_core::content::ContentHash;
use borax_core::record::Record;
use borax_core::template::{RenderInput, TemplateTable};
use borax_sources::store::write_atomically;

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
/// Rendered from `citation_keys` then stripped of
/// the characters a BibTeX key cannot carry — whitespace, and the
/// comma, braces, and percent sign that end a key, open or close a
/// group, or start a comment. `hash` supplies the `sha1` field a
/// template may use.
///
/// Returns `None` when the template renders nothing, or nothing that
/// survives the stripping — a record too sparse to cite.
pub fn citation_key(
    record: &Record,
    hash: Option<&ContentHash>,
    citation_keys: &TemplateTable,
) -> Option<String> {
    let key: String = citation_keys
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

/// The sidecar path for the file at `path`: the same path with
/// [`SIDECAR_EXTENSION`] appended.
///
/// Appended rather than substituted, so `paper.pdf` yields
/// `paper.pdf.bib`. Substituting would put the sidecar at `paper.bib`,
/// which is exactly the name a person keeping notes on `paper.pdf` by
/// hand would have chosen, and the sidecar namespace must not collide
/// with a namespace users already occupy.
///
/// A path with no extension gains the only one it has, so `paper`
/// yields `paper.bib`.
pub fn sidecar_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(SIDECAR_EXTENSION);
    path.with_file_name(name)
}

/// Whether the sidecar at `target` may be replaced.
///
/// True when nothing is there, and when what is there is recognisably a
/// sidecar borax wrote — one carrying a record under
/// [`borax_core::bib_output::parse_sidecar_record`]'s marker. A re-run
/// keeps its own sidecars current; it never touches anything else.
///
/// A target that cannot be read is treated as occupied. Not knowing what
/// is there is not a reason to overwrite it.
fn sidecar_is_ours(target: &Path, files: &dyn BibFiles) -> bool {
    match files.read(target) {
        Ok(existing) => existing.trim().is_empty() || parse_sidecar_record(&existing).is_some(),
        Err(_) => false,
    }
}

/// Write bibliography output for `resolved`.
///
/// [`write_sidecar`] per file in the order given, then [`merge_master`]
/// over every file that had a citation key, so a caller doing the two
/// halves itself — a sidecar as each file is reached, the merge once the
/// batch is done — writes the same files and reports the same outcomes.
///
/// Events come back in a fixed order — every sidecar event in input
/// order, then every master-file event in input order — so two runs
/// over the same inputs produce the same stream.
///
/// A read or write that fails is
/// [`crate::event::SkipReason::BibWriteFailed`] for the files it cost,
/// and the run continues: bibliography output is the last thing a run
/// does, and losing it takes nothing away from the renames that
/// preceded it.
pub fn write_bib(
    resolved: &[(PathBuf, FileRecord)],
    citation_keys: &TemplateTable,
    config: &BibConfig,
    files: &dyn BibFiles,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut keyed = Vec::new();

    for (path, file) in resolved {
        let (key, event) = write_sidecar(path, file, citation_keys, config, files);
        events.extend(event);
        if let Some(key) = key {
            keyed.push(Keyed {
                path,
                record: &file.record,
                key,
            });
        }
    }

    events.extend(merge_master(&keyed, config, files));
    events
}

/// Write the sidecar for the resolved file `file` at `path`, and answer
/// the key its record is cited under.
///
/// The key is [`citation_key`]'s, and `path` is where the file is now:
/// it is where the sidecar goes and what the events name, so a run that
/// has already moved the file gives the name it now carries.
///
/// A record that renders no key is [`SkipReason::Unciteable`], which is
/// the `None` key and that skip together: it has no place in either
/// destination.
///
/// A sidecar is written only when [`BibConfig::sidecars`] is set,
/// reported as [`Event::Sidecar`]. One that would overwrite something
/// borax did not write is [`SkipReason::SidecarTaken`], and one the
/// filesystem refuses is [`SkipReason::BibWriteFailed`]. The key comes
/// back regardless, since the master file is a separate destination and
/// losing one does not cost the other.
pub fn write_sidecar(
    path: &Path,
    file: &FileRecord,
    citation_keys: &TemplateTable,
    config: &BibConfig,
    files: &dyn BibFiles,
) -> (Option<String>, Option<Event>) {
    let Some(key) = citation_key(&file.record, file.hash.as_ref(), citation_keys) else {
        return (
            None,
            Some(Event::Skipped {
                path: path.to_path_buf(),
                reason: SkipReason::Unciteable,
            }),
        );
    };

    if !config.sidecars {
        return (Some(key), None);
    }

    let target = sidecar_path(path);
    let event = match sidecar_is_ours(&target, files) {
        false => Event::Skipped {
            path: path.to_path_buf(),
            reason: SkipReason::SidecarTaken { target },
        },
        true => match files.write(&target, &sidecar(&file.record, &key)) {
            Ok(()) => Event::Sidecar {
                path: path.to_path_buf(),
                target,
            },
            Err(error) => bib_failed(path, format!("writing \"{}\": {error}", target.display())),
        },
    };
    (Some(key), Some(event))
}

/// Merge `keyed` into the master `.bib` and report what became of each
/// entry.
///
/// Nothing is read or written when [`BibConfig::path`] names no master
/// file, and nothing when `keyed` is empty — a run with nothing to merge
/// leaves the file as it is rather than rewriting it byte for byte.
///
/// Otherwise the file is read, every entry merged into it in one pass
/// ([`borax_core::bib_output::merge`]), and the whole of it written back.
/// One pass is what lets the merge assign keys unique across everything
/// it is given rather than a file at a time. Each entry is an
/// [`Event::BibEntry`], in `keyed`'s order, naming the key it landed
/// under and the outcome — `added`, `already-present`, or `updated`.
///
/// A read or a write that fails is [`SkipReason::BibWriteFailed`] for
/// every entry it cost, naming the master file, and no entry is
/// reported twice.
pub fn merge_master(keyed: &[Keyed<'_>], config: &BibConfig, files: &dyn BibFiles) -> Vec<Event> {
    let Some(master) = &config.path else {
        return Vec::new();
    };
    if keyed.is_empty() {
        return Vec::new();
    }

    let existing = match files.read(master) {
        Ok(existing) => existing,
        Err(error) => {
            return keyed
                .iter()
                .map(|entry| {
                    bib_failed(
                        entry.path,
                        format!("reading \"{}\": {error}", master.display()),
                    )
                })
                .collect();
        }
    };

    let additions: Vec<(&str, &Record)> = keyed
        .iter()
        .map(|entry| (entry.key.as_str(), entry.record))
        .collect();
    let merged = merge(&existing, &additions, config.duplicates);

    match files.write(master, &merged.content) {
        Ok(()) => keyed
            .iter()
            .zip(&merged.outcomes)
            .map(|(entry, outcome)| entry_event(entry.path, outcome))
            .collect(),
        Err(error) => keyed
            .iter()
            .map(|entry| {
                bib_failed(
                    entry.path,
                    format!("writing \"{}\": {error}", master.display()),
                )
            })
            .collect(),
    }
}

/// One resolved file that has a citation key, and so a place in the
/// master `.bib`.
pub struct Keyed<'a> {
    /// Where the file is, which is what the events about it name.
    pub path: &'a Path,
    /// What the master file gets under `key`.
    pub record: &'a Record,
    /// The key the record is cited under, from [`citation_key`].
    pub key: String,
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

    /// Replace `path` through a temporary file in its own directory
    /// ([`borax_sources::store::write_atomically`]).
    ///
    /// A master `.bib` is a file the user has been accumulating, quite
    /// possibly for years, and a merge rewrites the whole of it. A
    /// truncating write would put every byte of it at the mercy of a
    /// full disk or a killed process for as long as the write takes;
    /// replacing it in one step means a run that dies leaves the
    /// previous bibliography exactly as it was.
    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        write_atomically(path, content.as_bytes())
    }
}
