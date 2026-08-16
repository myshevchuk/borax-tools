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

use std::io;
use std::path::{Path, PathBuf};

use borax_core::bib_output::DuplicatePolicy;
use borax_core::content::ContentHash;
use borax_core::record::Record;
use borax_core::template::TemplateTable;

use crate::event::Event;
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

/// The citation key `record` is cited under.
///
/// Rendered from `templates` exactly as a filename is, then stripped of
/// the characters a BibTeX key cannot carry, so a work is cited under
/// the same name its file is stored under. `hash` supplies the `sha1`
/// field a template may use.
///
/// Returns `None` when the template renders nothing, or nothing that
/// survives the stripping — a record too sparse to cite.
pub fn citation_key(
    record: &Record,
    hash: Option<&ContentHash>,
    templates: &TemplateTable,
) -> Option<String> {
    todo!("render the template and restrict it to key characters")
}

/// The sidecar path for the file at `path`: the same path with its
/// extension replaced by [`SIDECAR_EXTENSION`].
pub fn sidecar_path(path: &Path) -> PathBuf {
    todo!("swap the extension")
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
    todo!("key each record, write sidecars, merge the master file")
}

/// A [`BibFiles`] backed by the real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealBibFiles;

impl BibFiles for RealBibFiles {
    fn read(&self, path: &Path) -> io::Result<String> {
        todo!("read, mapping a missing file to empty content")
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        todo!("create the parent directories and write")
    }
}
