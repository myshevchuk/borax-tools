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
    match entry_type {
        EntryType::Article => "article",
        EntryType::Preprint => "misc",
        EntryType::Book => "book",
        EntryType::Chapter => "incollection",
        EntryType::Thesis => "thesis",
        EntryType::Report => "techreport",
        EntryType::Patent => "patent",
        EntryType::Standard => "standard",
    }
}

/// Escape a value for use inside a BibTeX field's braces.
///
/// The TeX special characters `& % $ # _ { }` gain a backslash;
/// `~`, `^`, and `\` become `\textasciitilde{}`, `\textasciicircum{}`,
/// and `\textbackslash{}`. Non-ASCII text passes through unchanged (the
/// emitted file is UTF-8, valid for BibLaTeX and UTF-8-aware BibTeX
/// engines). The transformation is not reversible.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => {
                escaped.push('\\');
                escaped.push(c);
            }
            '~' => escaped.push_str("\\textasciitilde{}"),
            '^' => escaped.push_str("\\textasciicircum{}"),
            '\\' => escaped.push_str("\\textbackslash{}"),
            _ => escaped.push(c),
        }
    }
    escaped
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
    let mut out = String::new();
    out.push('@');
    out.push_str(entry_kind(record.entry_type));
    out.push('{');
    out.push_str(key);
    out.push_str(",\n");

    if !record.authors.is_empty() {
        let authors: Vec<String> = record
            .authors
            .iter()
            .map(|name| match &name.given {
                Some(given) => format!("{}, {}", escape(&name.family), escape(given)),
                None => escape(&name.family),
            })
            .collect();
        push_field(&mut out, "author", &authors.join(" and "));
    }

    if let Some(title) = &record.title {
        push_field(&mut out, "title", &escape(title));
    }

    if let Some(container) = &record.container_title {
        match record.entry_type {
            EntryType::Article => push_field(&mut out, "journal", &escape(container)),
            EntryType::Chapter => push_field(&mut out, "booktitle", &escape(container)),
            _ => {}
        }
    }

    if let Some(issued) = &record.issued {
        push_field(&mut out, "year", &issued.year.to_string());
        if let Some(month) = issued.month {
            push_field(&mut out, "month", &month.to_string());
        }
    }

    if let Some(volume) = &record.volume {
        push_field(&mut out, "volume", volume);
    }
    if let Some(issue) = &record.issue {
        push_field(&mut out, "number", issue);
    }
    if let Some(pages) = &record.pages {
        push_field(&mut out, "pages", pages);
    }
    if let Some(publisher) = &record.publisher {
        push_field(&mut out, "publisher", &escape(publisher));
    }
    if let Some(doi) = &record.doi {
        push_field(&mut out, "doi", doi.as_str());
    }
    if let (EntryType::Preprint, Some(arxiv)) = (record.entry_type, &record.borax.arxiv) {
        push_field(&mut out, "eprint", arxiv.id());
        push_field(&mut out, "archiveprefix", "arXiv");
    }
    if let Some(pmid) = &record.pmid {
        push_field(&mut out, "pmid", &pmid.to_string());
    }
    if let Some(isbn) = &record.isbn {
        push_field(&mut out, "isbn", isbn.as_str());
    }

    out.push_str("}\n");
    out
}

/// Append one `  name = {value},` line, value taken verbatim.
fn push_field(out: &mut String, name: &str, value: &str) {
    out.push_str("  ");
    out.push_str(name);
    out.push_str(" = {");
    out.push_str(value);
    out.push_str("},\n");
}
