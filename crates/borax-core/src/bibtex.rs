//! Deterministic BibTeX/BibLaTeX emission.
//!
//! Emission is a pure function of (record, key): the same inputs always
//! produce byte-identical output. Values pass through [`escape`]; the
//! record's stored strings are otherwise emitted as they are (no page or
//! name reformatting beyond the BibTeX `and`-joined author list).

use crate::record::{EntryType, Record};

/// The BibTeX entry type a record emits as.
///
/// `Article` → `article`, `Preprint` → `misc` (with `eprint` /
/// `archiveprefix` fields when the record carries an arXiv id),
/// `Book` → `book`, `Chapter` → `incollection`, `Thesis` → `thesis`
/// (BibLaTeX; classic styles need a mapping), `Report` → `techreport`,
/// `Patent` → `patent` (BibLaTeX), `Standard` → `standard` (BibLaTeX).
pub fn entry_kind(entry_type: EntryType) -> &'static str {
    let _ = entry_type;
    todo!("map EntryType to its BibTeX entry kind")
}

/// Escape a value for use inside a BibTeX field's braces.
///
/// The TeX special characters `& % $ # _ { }` gain a backslash;
/// `~`, `^`, and `\` become `\textasciitilde{}`, `\textasciicircum{}`,
/// and `\textbackslash{}`. Non-ASCII text passes through unchanged (the
/// emitted file is UTF-8, valid for BibLaTeX and UTF-8-aware BibTeX
/// engines). The transformation is not reversible.
pub fn escape(text: &str) -> String {
    let _ = text;
    todo!("escape TeX special characters")
}

/// Emit one BibTeX entry for `record` under `key`.
///
/// Output shape (two-space indent, ` = ` separator, braced values, a
/// trailing comma after every field, one trailing newline):
///
/// ```text
/// @article{key,
///   author = {Family, Given and Family, Given},
///   title = {...},
/// }
/// ```
///
/// Fields appear in this fixed order, each omitted when the record
/// lacks it: `author`, `title`, `journal` (articles) / `booktitle`
/// (chapters), `year`, `month`, `volume`, `number`, `pages`,
/// `publisher`, `doi`, `eprint` + `archiveprefix` (preprints with an
/// arXiv id; `eprint` is the bare id without version), `pmid`, `isbn`.
/// Authors render as `Family, Given` joined by ` and `; `month` is the
/// numeric month when the record's date carries one.
pub fn emit(record: &Record, key: &str) -> String {
    let _ = (record, key);
    todo!("emit a deterministic BibTeX entry")
}
