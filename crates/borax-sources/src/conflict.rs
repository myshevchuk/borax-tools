//! Detecting that a resolved record does not describe the file.
//!
//! An identifier extracted from a PDF can be wrong — a DOI in a
//! reference list, a cover sheet from another paper, a copy-paste in
//! the metadata. When it is, resolution succeeds and returns a record
//! for the wrong work, which is exactly the failure renaming must never
//! commit. Comparing what the file said about itself against what the
//! service returned catches it.

use borax_core::record::Record;

/// A disagreement between the file and the resolved record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The CSL field that disagrees.
    pub field: &'static str,
    /// What the file's own metadata said.
    pub extracted: String,
    /// What the resolved record says.
    pub resolved: String,
}

/// Reduce a title to its comparison form: lowercase, every character
/// that is not alphanumeric or whitespace dropped, and whitespace runs
/// collapsed to single spaces with the ends trimmed.
///
/// The transformation is lossy: the result is a comparison key, not a
/// title. It exists so that typographic differences between a PDF's
/// embedded metadata and a publisher's record — curly versus straight
/// quotes, an em dash, a trailing period, LaTeX-ish spacing — do not
/// read as disagreement.
pub fn normalize_title(title: &str) -> String {
    let stripped: String = title
        .chars()
        .filter(|character| character.is_alphanumeric() || character.is_whitespace())
        .collect();
    stripped
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
        .to_lowercase()
}

/// Compare the title a file claimed with the one the resolved record
/// carries.
///
/// `None` — no conflict — when either title is missing or empty after
/// normalization (nothing to compare is not disagreement), when the
/// normalized forms are equal, or when one is a prefix of the other at
/// a word boundary. That last rule is what keeps a subtitle from
/// looking like a conflict: a PDF's metadata often holds
/// `"Molecular Structure of Nucleic Acids"` where the record holds
/// `"Molecular Structure of Nucleic Acids: A Structure for
/// Deoxyribose Nucleic Acid"`.
///
/// Otherwise `Some(Conflict)` carrying the two titles **as they were
/// given**, not normalized: a person reading the run summary needs to
/// see the real strings to judge what happened.
pub fn check_title(extracted: Option<&str>, record: &Record) -> Option<Conflict> {
    let extracted = extracted?;
    let resolved = record.title.as_deref()?;

    let normalized_extracted = normalize_title(extracted);
    let normalized_resolved = normalize_title(resolved);
    if normalized_extracted.is_empty() || normalized_resolved.is_empty() {
        return None;
    }

    if normalized_extracted == normalized_resolved
        || extends(&normalized_extracted, &normalized_resolved)
        || extends(&normalized_resolved, &normalized_extracted)
    {
        return None;
    }

    Some(Conflict {
        field: "title",
        extracted: extracted.to_string(),
        resolved: resolved.to_string(),
    })
}

/// Whether `longer` is `shorter` followed by a further word — a
/// subtitle, rather than a different title that happens to start with
/// the same letters.
fn extends(shorter: &str, longer: &str) -> bool {
    longer
        .strip_prefix(shorter)
        .is_some_and(|rest| rest.starts_with(' '))
}
