//! What an operator is shown before being asked about a file.
//!
//! A question names a file and a target, and the operator's real
//! question is whether the record behind that target is the right one.
//! A plausible name is as easy to render from the wrong record as from
//! the right one — a DOI picked out of a bibliography resolves to a
//! real paper, just not this one — so the evidence has to be on the
//! screen when the question is.
//!
//! The description is a pure function of a file's `resolved` event and
//! the move being proposed: it invents nothing, and shows nothing about
//! the resolution that the event stream does not also carry.

use borax_core::record::{DateParts, EntryType, Name, Record};

use crate::event::{Attempt, Claim, ClaimOrigin, Event, SkipReason};

/// The move a description is about: where the file would go, and the
/// name the template rendered before a collision moved it aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// The name the file would take, relative to its own directory,
    /// including any collision suffix.
    pub target: String,
    /// The name the template rendered, when a collision suffix means
    /// the file is not taking it, and `None` when the target is what
    /// was rendered.
    pub rendered: Option<String>,
}

/// One file's position among the run's files, counted from one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub of_this: usize,
    pub total: usize,
}

/// The width a description wraps to when the terminal does not say.
pub const DEFAULT_WIDTH: usize = 80;

/// The columns a label occupies, its value starting at the next one.
///
/// Two columns wider than `identifier`, the longest of them, so that no
/// label runs into its value.
const LABEL: usize = 12;

/// The columns left free at the right of a wrapped line.
///
/// A description is read beside a menu rather than alone, and a value
/// stopping short of the last column stays one line when the window is
/// a little narrower than it said, or when something else shares the
/// terminal. Only wrapping observes it: the rule fills the width, since
/// a rule that stopped short would read as a shorter rule rather than
/// as a margin.
const MARGIN: usize = 4;

/// How many authors are named before the rest become a count.
const NAMED_AUTHORS: usize = 3;

/// The lines shown above a question about `resolved`, proposing
/// `proposal`, or naming no new name when there is no proposal to
/// make — a file the run could not identify has nothing to be
/// renamed to.
///
/// `resolved` is the file's [`Event::Resolved`]; any other event has no
/// description and yields no lines.
///
/// `name` is what the file is called on the `file` line. The description
/// prints it and does not work it out: what to call a file is the run's
/// to settle, and the run is the one that knows where it was started.
///
/// The lines, in order, leaving out every field the record does not
/// hold: a rule carrying `position`; `name`; the identifier
/// the run looked up and where it was found; the services that
/// supplied the record; its type, title, authors, date of issue and
/// container; the titles the file claims for itself with where each was
/// read; and the name the file would take, with a line saying which
/// rendered name was taken when a suffix moved it aside.
///
/// The rule fills `width`; values wrap within it, under a hanging
/// indent as wide as the label column. The identifier is the exception
/// and is written whole however long it runs, since one broken across
/// two lines cannot be read back. Nothing is truncated except the list
/// of authors, which gives three and a count: a title cut short is what
/// would hide the difference between two works.
pub fn describe(
    resolved: &Event,
    name: &str,
    proposal: Option<&Proposal>,
    position: Position,
    width: usize,
) -> Vec<String> {
    let mut description = Description {
        lines: vec![rule(position, width)],
        width,
    };
    description.field("file", name);

    let Event::Resolved {
        record,
        source,
        found,
        claims,
        tier,
        overrode,
        cached,
        ..
    } = resolved
    else {
        if let Event::Skipped { reason, .. } = resolved {
            failure(&mut description, reason);
            return description.lines;
        }
        return Vec::new();
    };
    if !found.is_empty() {
        // Whole, however long it runs. An identifier folded across two
        // lines cannot be read back or copied out, and it is the one
        // value in the description a person takes away with them.
        //
        // The clause saying where it was found is written only where
        // the run found it. A content-index answer looked nothing up:
        // the index keeps records rather than the identifiers they
        // were reached by, so a record reached last time by an arXiv
        // identifier and carrying a DOI has nothing to say about where
        // the DOI came from, and saying "from an earlier run" would
        // say something false about it.
        description.whole(
            "identifier",
            &match whence(tier.as_deref()) {
                Some(whence) => format!("{found}, {whence}"),
                None => found.clone(),
            },
        );
    }
    description.field("record", &record_from(source, *cached));
    description.field("type", type_name(record.entry_type));
    if let Some(title) = held(record.title.as_deref()) {
        description.field("title", title);
    }
    if !record.authors.is_empty() {
        description.field("authors", &authors(&record.authors));
    }
    if let Some(issued) = &record.issued {
        description.field("issued", &issued_on(issued));
    }
    if let Some(within) = within(record) {
        description.field("in", &within);
    }
    match claims.split_first() {
        None => description.field("file says", "nothing read"),
        Some((first, rest)) => {
            description.field("file says", &claimed(first));
            for claim in rest {
                description.field("", &claimed(claim));
            }
        }
    }
    if let Some(overrode) = overrode {
        description.field("conflict", &alike(&overrode.field, overrode.similarity));
    }
    if let Some(proposal) = proposal {
        description.field("new name", &proposal.target);
        if let Some(rendered) = &proposal.rendered {
            description.field("", &format!("({rendered} is taken)"));
        }
    }

    description.lines
}

/// The lines of one description, as the fields are laid into them.
struct Description {
    lines: Vec<String>,
    width: usize,
}

impl Description {
    /// Write `value` against `label`, wrapped within the width and
    /// indented under the label column on every line after the first.
    ///
    /// An empty `label` continues the field above, which is how a
    /// second claimed title and the note about a taken name are
    /// written.
    fn field(&mut self, label: &str, value: &str) {
        self.lines
            .extend(wrapped(label, &escaped(value), self.width));
    }

    /// Write `value` against `label` on one line, whatever the width.
    fn whole(&mut self, label: &str, value: &str) {
        self.lines.push(format!(
            "{label}{}{}",
            " ".repeat(LABEL.saturating_sub(label.chars().count())),
            escaped(value)
        ));
    }
}

/// The rule a description opens with: the position, then dashes to the
/// whole of `width`.
fn rule(position: Position, width: usize) -> String {
    let opening = format!("── {} of {} ", position.of_this, position.total);
    let dashes = width.saturating_sub(opening.chars().count());
    format!("{opening}{}", "─".repeat(dashes))
}

/// `value` written against `label` and folded to fit `width`, every
/// line after the first indented under the label column.
fn wrapped(label: &str, value: &str, width: usize) -> Vec<String> {
    let room = width.saturating_sub(LABEL + MARGIN).max(1);
    let indent = " ".repeat(LABEL);
    fold(value, room)
        .into_iter()
        .enumerate()
        .map(|(line, text)| match (line, label.is_empty()) {
            (0, false) => format!(
                "{label}{}{text}",
                " ".repeat(LABEL.saturating_sub(label.chars().count()))
            ),
            _ => format!("{indent}{text}"),
        })
        .collect()
}

/// `text` broken into lines of at most `room` characters, at the spaces
/// between words, and at `room` itself for a word with no space to
/// break at.
///
/// A description truncates nothing, so an overlong word is carried
/// across lines rather than cut short — but it is carried, not left to
/// run past the width and break the layout around it. A file name with
/// no spaces in it is the usual one.
fn fold(text: &str, room: usize) -> Vec<String> {
    let room = room.max(1);
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= room => {
                line.push(' ');
                line.push_str(word);
            }
            _ => {
                let mut rest: Vec<char> = word.chars().collect();
                while rest.len() > room {
                    lines.push(rest.drain(..room).collect());
                }
                lines.push(rest.into_iter().collect());
            }
        }
    }
    match lines.is_empty() {
        true => vec![String::new()],
        false => lines,
    }
}

/// `text` with every control character written out rather than sent to
/// the terminal.
///
/// A title, a container, an author's name: all of it is written by
/// whoever made the file or the record, and none of it is borax's. An
/// escape sequence left in one would be acted on by the terminal the
/// description is drawn on — `ESC [2J` erases the screen — and what it
/// could redraw is the file and the target above a menu whose first
/// choice is Rename. The operator would then answer about a file they
/// were never shown.
///
/// Whitespace is left alone: folding has already made the lines, and a
/// space is not a command.
fn escaped(text: &str) -> String {
    text.chars()
        .map(|character| match character.is_control() {
            true => format!("\\x{:02x}", character as u32),
            false => character.to_string(),
        })
        .collect()
}

/// Where the identifier was found, as the clause following it.
///
/// A file that was not opened has no pass to name, and the identifier
/// it was filed under is what an earlier run found.
fn whence(tier: Option<&str>) -> Option<&'static str> {
    match tier {
        Some("embedded-metadata") => Some("from embedded metadata"),
        Some("text-layer") => Some("from the text layer"),
        // Not "supplied by hand" or "supplied by you": every other
        // value in this slot names where the identifier was read, and
        // this one names that it was not read at all.
        Some("supplied") => Some("supplied"),
        Some(_) => Some("from the file"),
        // Nothing was looked up, so nothing is known about where the
        // record's identifier came from. [`record_from`] says what is
        // known: the record itself is from an earlier run.
        None => None,
    }
}

/// The lines describing a verdict that identified nothing, written
/// after the `file` line.
///
/// A failed verdict carries less than a resolution does, and the
/// description shows what it carries and no more: a `skipped` event
/// holds a path and a reason, so there is no record to lay out and no
/// name to propose. What the operator is being asked to judge is why
/// the file got no further, which is exactly what the reason holds.
fn failure(description: &mut Description, reason: &SkipReason) {
    match reason {
        // The identifier is the thing a person asked to supply a better
        // one has to improve on, so it leads — and whole, as on a
        // resolution.
        SkipReason::Unresolvable { found, attempts } => {
            description.whole("identifier", found);
            match attempts.split_first() {
                None => description.field("no record", "no source was asked"),
                Some((first, rest)) => {
                    description.field("no record", &answered(first));
                    for attempt in rest {
                        description.field("", &answered(attempt));
                    }
                }
            }
        }
        // The same slot as an identifier that was found: its question
        // is what was looked up, and the answer here is nothing.
        SkipReason::NoIdentifier => {
            description.field("identifier", "none found in the file");
        }
        // Both titles are already labelled on a resolution's layout, so
        // a conflict borrows those two labels and adds only how close
        // the two were.
        SkipReason::Conflict {
            field,
            extracted,
            resolved,
            similarity,
        } => {
            description.field("title", resolved);
            description.field("file says", extracted);
            description.field("conflict", &alike(field, *similarity));
        }
        SkipReason::Unreadable { message } => {
            description.field("unreadable", message);
        }
        SkipReason::TargetTaken { target } => {
            description.field("name taken", &target.display().to_string());
        }
        SkipReason::Unnameable => {
            description.field("no name", "the record is too sparse to name a file");
        }
        // Every other reason is either an outcome no question follows
        // (a duplicate, an unrecordable file) or a failure after a
        // decision was already made. Naming the file is all the
        // description has to say about it.
        _ => {}
    }
}

/// One service's answer, as the `no record` block lists it.
fn answered(attempt: &Attempt) -> String {
    format!("{}: {}", attempt.source, attempt.error)
}

/// How close two values were, as the `conflict` line puts it.
///
/// A percentage rather than the stored fraction: the number is being
/// read by a person deciding whether two titles are the same work, and
/// `8% alike` is a judgement they can make where `0.08` is a value they
/// have to convert first.
fn alike(field: &str, similarity: f64) -> String {
    format!("{field}s {}% alike", (similarity * 100.0).round())
}

/// The services line: who supplied the record, and whether it was
/// asked this time or kept from before.
fn record_from(source: &str, cached: bool) -> String {
    let named = services(source);
    match cached && !named.is_empty() {
        true => format!("{named}, from an earlier run"),
        false => named,
    }
}

/// The services an event's `source` names, as a person reads them.
///
/// The event writes them lowercase and comma-separated, which is the
/// form a `--json` consumer matches on; here each is spelled as its
/// service spells itself, and the content index answering for a record
/// whose makers are unknown is an earlier run rather than a cache the
/// operator has no reason to think about.
fn services(source: &str) -> String {
    source
        .split(", ")
        .map(|service| match service {
            "crossref" => "Crossref",
            "openalex" => "OpenAlex",
            "arxiv" => "arXiv",
            "datacite" => "DataCite",
            "pubmed" => "PubMed",
            "sidecar" => "a sidecar",
            "cache" => "an earlier run",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The document type, named as the project names it in prose rather
/// than in the CSL string a record serializes to.
fn type_name(entry_type: EntryType) -> &'static str {
    match entry_type {
        EntryType::Article => "journal article",
        EntryType::Preprint => "preprint",
        EntryType::Book => "book",
        EntryType::Chapter => "chapter",
        EntryType::Thesis => "thesis",
        EntryType::Report => "report",
        EntryType::Patent => "patent",
        EntryType::Standard => "standard",
    }
}

/// The authors, up to [`NAMED_AUTHORS`] of them, and a count of the
/// rest.
///
/// A list of names here is evidence of whose paper this is rather than
/// a bibliography, and the first few settle that as well as all of them
/// would.
fn authors(authors: &[Name]) -> String {
    let named: Vec<String> = authors.iter().take(NAMED_AUTHORS).map(full_name).collect();
    match authors.len().saturating_sub(NAMED_AUTHORS) {
        0 => named.join(", "),
        rest => format!("{}, and {rest} more", named.join(", ")),
    }
}

/// One author, given name first, or the family name alone where the
/// record holds nothing else.
fn full_name(name: &Name) -> String {
    match &name.given {
        Some(given) => format!("{given} {}", name.family),
        None => name.family.clone(),
    }
}

/// The date of issue, to whatever precision the record holds it.
fn issued_on(issued: &DateParts) -> String {
    match (issued.month, issued.day) {
        (Some(month), Some(day)) => format!("{}-{month:02}-{day:02}", issued.year),
        (Some(month), None) => format!("{}-{month:02}", issued.year),
        _ => issued.year.to_string(),
    }
}

/// Where the work appeared: the containing title, its volume and issue,
/// and the pages within it, as far as the record has them.
///
/// `None` when the record names no container, which is what a preprint
/// or a book has instead of a journal: a volume with nothing to be a
/// volume of says nothing.
fn within(record: &Record) -> Option<String> {
    let mut line = held(record.container_title.as_deref())
        .unwrap_or_default()
        .to_string();
    if let Some(volume) = held(record.volume.as_deref()) {
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(volume);
    }
    if let Some(issue) = held(record.issue.as_deref()) {
        line.push_str(&format!("({issue})"));
    }
    if let Some(pages) = held(record.pages.as_deref()) {
        let separator = match line.is_empty() {
            true => "",
            false => ", ",
        };
        line.push_str(&format!("{separator}{pages}"));
    }
    (!line.is_empty()).then_some(line)
}

/// `text` when the record really holds it, and `None` when what it
/// holds is nothing to show.
///
/// A source can answer with an empty string or a run of spaces where it
/// has no value, and a field a record does not hold is left out rather
/// than printed as a label with nothing after it.
fn held(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.trim().is_empty())
}

/// One title the file claims, with where it was read.
fn claimed(claim: &Claim) -> String {
    format!(
        "{} ({})",
        claim.title,
        match claim.from {
            ClaimOrigin::Xmp => "XMP",
            ClaimOrigin::Info => "document info",
        }
    )
}
