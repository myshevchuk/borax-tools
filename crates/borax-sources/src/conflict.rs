//! Detecting that a resolved record does not describe the file.
//!
//! An identifier extracted from a PDF can be wrong — a DOI in a
//! reference list, a cover sheet from another paper, a copy-paste in
//! the metadata. When it is, resolution succeeds and returns a record
//! for the wrong work, which is exactly the failure renaming must never
//! commit. Comparing what the file said about itself against what the
//! service returned catches it.
//!
//! The comparison is deliberately generous, because the two errors do
//! not cost the same. A title the check wrongly calls a disagreement
//! costs a correct rename on a file the caller named, silently. A
//! disagreement it wrongly lets through costs a wrong name the caller
//! can correct by resolving again. So a file is cleared unless the
//! titles are
//! *materially* different, and the margin between "one letter a PDF
//! producer could not encode" and "a different work" is wide enough to
//! hold that.

use std::collections::BTreeMap;

use borax_core::identifier::{ArxivId, Doi};
use borax_core::record::Record;
use borax_core::template::{FUNCTION_WORDS, transliterate};

/// A disagreement between the file and the resolved record.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// The CSL field that disagrees.
    pub field: &'static str,
    /// What the file's own metadata said.
    pub extracted: String,
    /// What the resolved record says.
    pub resolved: String,
    /// How close the two were, on [`title_similarity`]'s scale. Always
    /// below [`TITLE_AGREEMENT`], and reported so a person judging the
    /// skip can see whether it was a near miss or nothing alike.
    pub similarity: f64,
}

/// The similarity at or above which two titles are taken to name the
/// same work.
///
/// Calibrated against real pairs rather than derived: a title whose
/// only difference is a character the PDF producer dropped scores near
/// 0.9, and two unrelated works in one field score near 0.2. The
/// threshold sits in the empty space between.
pub const TITLE_AGREEMENT: f64 = 0.6;

/// The longest a candidate may be while still being dismissed as a
/// producer's default rather than read as a competing title.
const SHORT_TITLE_TOKENS: usize = 3;

/// Titles that name no work, whatever a producer wrote them for.
const PLACEHOLDER_TITLES: [&str; 5] = [
    "untitled",
    "unknown",
    "no title",
    "document",
    "presentation",
];

/// Extensions that mark a candidate as a filename rather than a title.
const DOCUMENT_EXTENSIONS: [&str; 9] = [
    ".doc", ".docx", ".ppt", ".pptx", ".indd", ".qxd", ".tex", ".rtf", ".odt",
];

/// Prefixes producers put in front of a source filename.
const FILENAME_PREFIXES: [&str; 2] = ["microsoft word - ", "microsoft powerpoint - "];

/// Reduce a title to the content words two renderings of it should
/// share.
///
/// The pass folds both sides toward the lossiest representation either
/// could have, because the file's metadata and the service's record are
/// rarely encoded alike:
///
/// 1. Common Latin letters fold to ASCII
///    ([`borax_core::template::transliterate`]), so `Grüße` and
///    `Gruesse` meet.
/// 2. Every character that is not ASCII alphanumeric becomes a
///    separator. This covers punctuation, every width of dash, and also
///    the letters transliteration has no entry for — a Greek `α` is
///    dropped exactly as the PDF producers that cannot encode it drop
///    it.
/// 3. What remains is lowercased and split on whitespace.
/// 4. [`borax_core::template::FUNCTION_WORDS`] are removed, so a shared
///    `of` cannot prop up a comparison.
///
/// The transformation is lossy: the result is a comparison key, from
/// which the title cannot be recovered. A title of nothing but function
/// words yields an empty list.
pub fn comparison_tokens(title: &str) -> Vec<String> {
    transliterate(title)
        .chars()
        .map(|character| match character.is_ascii_alphanumeric() {
            true => character.to_ascii_lowercase(),
            false => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .filter(|word| !FUNCTION_WORDS.contains(word))
        .map(str::to_string)
        .collect()
}

/// How much two token lists overlap, from 0.0 (nothing in common) to
/// 1.0 (the same tokens the same number of times).
///
/// The Sørensen–Dice coefficient over token multisets:
/// `2·|A ∩ B| / (|A| + |B|)`, counting a token repeated in both sides
/// as often as the shorter side repeats it. Two empty lists score 1.0.
///
/// Length differences count against the score, which is what keeps a
/// short generic title from matching a long specific one. The
/// legitimate reason for a length difference — a subtitle present on
/// one side — is handled by [`titles_agree`] before the score is
/// consulted.
pub fn title_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }

    let mut remaining: BTreeMap<&str, usize> = BTreeMap::new();
    for token in a {
        *remaining.entry(token.as_str()).or_default() += 1;
    }

    let shared = b
        .iter()
        .filter(|token| match remaining.get_mut(token.as_str()) {
            Some(count) if *count > 0 => {
                *count -= 1;
                true
            }
            _ => false,
        })
        .count();

    2.0 * shared as f64 / (a.len() + b.len()) as f64
}

/// Whether two token lists name the same work.
///
/// Any one of four rules clears them, in the order tried:
///
/// 1. Either list is empty. Nothing to compare is not disagreement.
/// 2. One list is a prefix of the other, which is a subtitle: a record
///    holding `Molecular Structure of Nucleic Acids: A Structure for
///    Deoxyribose Nucleic Acid` against a file holding the first half.
/// 3. The lists concatenated without separators are equal. A producer
///    that writes `ShelfStable` where a service writes `Shelf-Stable`
///    has split the same title differently, not named another work.
/// 4. [`title_similarity`] reaches [`TITLE_AGREEMENT`].
pub fn titles_agree(a: &[String], b: &[String]) -> bool {
    if a.is_empty() || b.is_empty() {
        return true;
    }
    if a.starts_with(b) || b.starts_with(a) {
        return true;
    }
    if a.concat() == b.concat() {
        return true;
    }
    title_similarity(a, b) >= TITLE_AGREEMENT
}

/// Whether `candidate` says anything about which work the file is.
///
/// A PDF's title field is as often a producer's leftover as a real
/// title, and a leftover compared against a record disagrees with it
/// every time. Treating one as evidence turns a resolvable file into a
/// permanent skip, so a candidate has to look like a title before it is
/// allowed to contradict one.
///
/// Rejected: a candidate with no content words; one of a handful of
/// placeholder titles; a filename, by its extension or by the prefix a
/// producer put in front of it; a bare DOI or arXiv identifier; and —
/// the rule that catches the rest — a candidate of at most
/// [`SHORT_TITLE_TOKENS`] content words sharing none of them with the
/// record.
///
/// That last rule is why no list of localized producer defaults is
/// needed: `PowerPoint-Präsentation` and `Diapositive 1` are short and
/// share nothing with any real title, while a short title that overlaps
/// the record is kept, and so is a long one that does not.
pub fn is_title_evidence(candidate: &str, record: &Record) -> bool {
    let tokens = comparison_tokens(candidate);
    if tokens.is_empty() {
        return false;
    }

    let trimmed = candidate.trim().to_lowercase();
    if PLACEHOLDER_TITLES.contains(&trimmed.as_str())
        || DOCUMENT_EXTENSIONS
            .iter()
            .any(|extension| trimmed.ends_with(extension))
        || FILENAME_PREFIXES
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
        || Doi::parse(candidate.trim()).is_ok()
        || ArxivId::parse(candidate.trim()).is_ok()
    {
        return false;
    }

    let resolved = record.title.as_deref().map(comparison_tokens);
    match resolved {
        Some(resolved) if tokens.len() <= SHORT_TITLE_TOKENS => {
            tokens.iter().any(|token| resolved.contains(token))
        }
        _ => true,
    }
}

/// Compare the titles a file claimed with the one the resolved record
/// carries.
///
/// `candidates` are the file's own title fields, in no particular
/// order; a document may carry several, and they need not agree with
/// each other. Each is judged by [`is_title_evidence`] and the rest
/// discarded, then compared by [`titles_agree`].
///
/// `None` — no conflict — when no candidate survives as evidence, when
/// the record has no title, or when **any** surviving candidate agrees.
/// One source of evidence agreeing is enough: a document whose XMP
/// packet holds a producer's default and whose Info dictionary holds
/// the real title has told the truth once, and once is what matters.
///
/// Otherwise `Some(Conflict)` describing the *closest* candidate,
/// carrying the two titles **as they were given** — a person reading
/// the run summary needs the real strings to judge what happened — and
/// the similarity that fell short.
pub fn check_title(candidates: &[&str], record: &Record) -> Option<Conflict> {
    let resolved = record.title.as_deref()?;
    let resolved_tokens = comparison_tokens(resolved);
    if resolved_tokens.is_empty() {
        return None;
    }

    let mut closest: Option<(f64, &str)> = None;
    for candidate in candidates {
        if !is_title_evidence(candidate, record) {
            continue;
        }

        let tokens = comparison_tokens(candidate);
        if titles_agree(&tokens, &resolved_tokens) {
            return None;
        }

        let similarity = title_similarity(&tokens, &resolved_tokens);
        if closest.is_none_or(|(best, _)| similarity > best) {
            closest = Some((similarity, candidate));
        }
    }

    let (similarity, extracted) = closest?;
    Some(Conflict {
        field: "title",
        extracted: extracted.to_string(),
        resolved: resolved.to_string(),
        similarity,
    })
}
