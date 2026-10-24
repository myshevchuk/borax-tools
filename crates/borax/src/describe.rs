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

use std::path::Path;

use borax_core::record::{DateParts, EntryType, Name, Record};

use crate::event::{
    Acceptance, Claim, ClaimOrigin, Event, ExtractionResultStep, IdentifierOrigin, IndexReadStep,
    LibraryAnswer, LookupStep, MatchCheckStep, RefreshAcceptance, RefreshStep, RetrievedFrom,
    Sections, ServiceAnswer, SkipReason, TitlesStep, answered_by, disagreement, referenced_root,
    references_read, rejected_identifiers, services_of, why_unanswered, why_unreachable,
};

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
/// `resolved` is the file's [`Event::Resolved`], or the `skipped` event
/// of a verdict that identified nothing; any other event has no
/// description and yields no lines. Everything shown is read from the
/// event's own fields and sections.
///
/// `name` is what the file is called on the `file` line. The description
/// prints it and does not work it out: what to call a file is the run's
/// to settle, and the run is the one that knows where it was started.
///
/// The lines of a resolution, in order, leaving out every field the
/// record does not hold: a rule carrying `position`; `name`; the
/// identifier the run looked up and where it came from (the extraction
/// pass, `supplied`, or the earlier run the content index has it from),
/// or the record's own identifier where nothing was looked up; the
/// services that supplied the record, and whether it came from the
/// library or from another library a content-index reference names;
/// why the library could not answer for the file, where it could not;
/// a `reference` line for each content-index reference to a library
/// that could not answer, and a `references` line naming what each
/// disagreeing one names; the record's type, title,
/// authors, date of issue and container; what the file's own titles
/// say — each title with where it was read, or that it claims none,
/// could not be opened, or was not read and why; in a run given
/// `--refresh`, the `refresh` lines, and for a fresh record that
/// differs from the library item's its `differs` and `decision` lines;
/// the conflict an operator accepted the record over, or that a pending
/// candidate carries; a `rejected` line for each candidate the operator rejected,
/// in submission order, its identifier written whole; the `candidate`
/// line, while the record is an operator's candidate pending an answer;
/// and the name the file would take, with a line saying which rendered
/// name was taken when a suffix moved it aside.
///
/// The lines of a skip, after `name`: why the verdict got no further
/// (the extraction failure, the lookup and what each service answered,
/// or the two titles of a conflict), what the file's own titles say
/// wherever the verdict is not a conflict, why the library could not
/// answer, where it could not, the `reference` and `references` lines,
/// and then a `rejected` line for each candidate the operator rejected.
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
        identifier,
        record,
        sections,
        refresh,
        ..
    } = resolved
    else {
        if let Event::Skipped {
            reason, sections, ..
        } = resolved
        {
            failure(&mut description, reason, sections.as_deref());
            unanswered(
                &mut description,
                sections
                    .as_deref()
                    .and_then(|sections| sections.library.answer()),
            );
            if let Some(sections) = sections {
                unreferenced(&mut description, sections);
                rejected(&mut description, sections);
            }
            return description.lines;
        }
        return Vec::new();
    };
    // Whole, however long it runs. An identifier folded across two
    // lines cannot be read back or copied out, and it is the one value
    // in the description a person takes away with them.
    //
    // Where nothing was looked up, the record's own identifier is named
    // with no clause.
    match looked_up(sections) {
        Some(looked_up) => description.whole("identifier", &looked_up),
        None if !identifier.is_empty() => description.whole("identifier", identifier),
        None => {}
    }
    if let Some(from) = record_from(record, sections.record_retrieval.as_ref()) {
        description.field("record", &from);
    }
    unanswered(&mut description, sections.library.answer());
    unreferenced(&mut description, sections);
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
    file_says(&mut description, &sections.extraction.titles);
    if let Some(refresh) = refresh {
        refreshed(&mut description, refresh, sections);
    }
    if let (
        Acceptance::Overridden | Acceptance::Pending,
        MatchCheckStep::Conflict {
            field, similarity, ..
        },
    ) = (sections.acceptance, &sections.match_check)
    {
        description.field("conflict", &alike(field, *similarity));
    }
    rejected(&mut description, sections);
    if sections.acceptance == Acceptance::Pending {
        description.field("candidate", "pending; skipping leaves the file as it was");
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
///
/// Reachable from the run for the same reason: text the operator
/// pasted is quoted back to them when it names no identifier, and a
/// paste carries whatever was copied.
pub(crate) fn escaped(text: &str) -> String {
    text.chars()
        .map(|character| match character.is_control() {
            true => format!("\\x{:02x}", character as u32),
            false => character.to_string(),
        })
        .collect()
}

/// The identifier `sections` says was looked up, with where it came
/// from — `from embedded metadata` or `from the text layer` for the
/// file's own, as extraction's result names the pass; `supplied` for an
/// operator's; and for the content index's, as the evidence on its read
/// names its origin, `found by an earlier run`, `supplied in an earlier
/// run` or `accepted over a title conflict in an earlier run` — or
/// `None` when nothing was looked up.
fn looked_up(sections: &Sections) -> Option<String> {
    let (identifier, origin) = match &sections.lookup {
        LookupStep::Attempted {
            identifier, origin, ..
        }
        | LookupStep::NoEligibleService {
            identifier, origin, ..
        } => (identifier, *origin),
        LookupStep::NotAttempted { .. } => return None,
    };
    let whence = match (origin, &sections.extraction.result) {
        // Not "supplied by hand" or "supplied by you": every other
        // value in this slot names where the identifier was read, and
        // this one names that it was not read at all.
        (IdentifierOrigin::Operator, _) => Some("supplied"),
        (IdentifierOrigin::Extracted, ExtractionResultStep::Found { tier, .. }) => {
            Some(match tier.as_str() {
                "embedded-metadata" => "from embedded metadata",
                "text-layer" => "from the text layer",
                _ => "from the file",
            })
        }
        (IdentifierOrigin::Extracted, _) => None,
        (IdentifierOrigin::ContentIndex, _) => match &sections.content_index.read {
            IndexReadStep::Hit {
                evidence: Some(evidence),
                ..
            } => match evidence.origin.as_str() {
                "extracted" => Some("found by an earlier run"),
                "supplied" => Some("supplied in an earlier run"),
                "overridden" => Some("accepted over a title conflict in an earlier run"),
                _ => None,
            },
            _ => None,
        },
    };
    Some(match whence {
        Some(whence) => format!("{identifier}, {whence}"),
        None => identifier.clone(),
    })
}

/// The lines of a refresh of the library item the record came from:
///
/// - a `refresh` line naming the identifier looked up, written whole,
///   and a line under it saying what the services said: that the fresh
///   record does or does not differ from the library's, that no
///   configured service can be asked, or each service's answer, one to
///   a line; for an item with no identifier, one `refresh` line saying
///   so;
/// - for a fresh record that differs, a `differs` line per difference,
///   giving the library's value and the service's, and a `decision`
///   line saying what became of it, the library that can take it named
///   by the root `sections` gives.
fn refreshed(description: &mut Description, refresh: &RefreshStep, sections: &Sections) {
    let (identifier, attempts) = match refresh {
        RefreshStep::NotAttempted { .. } => {
            description.field(
                "refresh",
                "the library's record carries no identifier to ask about",
            );
            return;
        }
        RefreshStep::NoEligibleService { identifier } => (identifier, &[][..]),
        RefreshStep::Unresolved {
            identifier,
            attempts,
        }
        | RefreshStep::Current {
            identifier,
            attempts,
        }
        | RefreshStep::Differs {
            identifier,
            attempts,
            ..
        } => (identifier, attempts.as_slice()),
    };
    description.whole("refresh", identifier);
    let service = service_name(answered_by(attempts));
    match refresh {
        RefreshStep::NotAttempted { .. } => {}
        RefreshStep::NoEligibleService { .. } => {
            description.field("", "no configured service can be asked about it");
        }
        RefreshStep::Unresolved { attempts, .. } => {
            for attempt in attempts {
                description.field("", &attempt.said());
            }
        }
        RefreshStep::Current { .. } => {
            description.field(
                "",
                &format!("{service} does not differ from the library's record"),
            );
        }
        RefreshStep::Differs {
            differences,
            acceptance,
            ..
        } => {
            description.field("", &format!("{service} differs from the library's record"));
            for difference in differences {
                let value = |value: Option<&serde_json::Value>| {
                    value.map_or("nothing".to_string(), |value| {
                        written(&difference.field, value)
                    })
                };
                description.field(
                    "differs",
                    &format!(
                        "{}: {} in the library, {} from {service}",
                        difference.field,
                        value(difference.item.as_ref()),
                        value(difference.candidate.as_ref())
                    ),
                );
            }
            description.field(
                "decision",
                &match acceptance {
                    RefreshAcceptance::Pending => {
                        "pending; keeping it leaves the library's record as it is".to_string()
                    }
                    RefreshAcceptance::Accepted => "the library's record was replaced".to_string(),
                    RefreshAcceptance::Unwritten { message } => {
                        format!("the library's record could not be replaced ({message})")
                    }
                    RefreshAcceptance::Declined => "the library's record was kept".to_string(),
                    RefreshAcceptance::NotAttempted { reason } => match reason.as_str() {
                        "another-library" => format!(
                            "only a rename in the library at {} can take it",
                            referenced_root(sections)
                        ),
                        "not-recording" => {
                            "this run records nothing, so the library's record stays as it is"
                                .to_string()
                        }
                        "already-asked" => {
                            "answered at an earlier file of the same item".to_string()
                        }
                        _ => "the library's record is unchanged".to_string(),
                    },
                },
            );
        }
    }
}

/// One side of a refresh's difference in `field`, as a `differs` line
/// writes it: the type as the `type` line names it, every author given
/// name first, the date as the `issued` line writes it, and any other
/// text in double quotes. A value that does not read as its field's is
/// written as the record serializes it.
fn written(field: &str, value: &serde_json::Value) -> String {
    let parsed = match field {
        "type" => serde_json::from_value::<EntryType>(value.clone())
            .ok()
            .map(|entry_type| type_name(entry_type).to_string()),
        "author" => serde_json::from_value::<Vec<Name>>(value.clone())
            .ok()
            .map(|names| names.iter().map(full_name).collect::<Vec<_>>().join(", ")),
        "issued" => serde_json::from_value::<DateParts>(value.clone())
            .ok()
            .map(|issued| issued_on(&issued)),
        _ => None,
    };
    parsed.unwrap_or_else(|| match value {
        serde_json::Value::String(text) => format!("\"{text}\""),
        other => other.to_string(),
    })
}

/// The `file says` lines: each title the file claims with where it was
/// read, or the state its titles are in when there is none to show.
fn file_says(description: &mut Description, titles: &TitlesStep) {
    match titles {
        TitlesStep::Read { claims } => match claims.split_first() {
            None => description.field("file says", "no title in its metadata"),
            Some((first, rest)) => {
                description.field("file says", &claimed(first));
                for claim in rest {
                    description.field("", &claimed(claim));
                }
            }
        },
        TitlesStep::Failed { message } => {
            description.field("file says", &format!("could not be opened ({message})"));
        }
        TitlesStep::NotAttempted { reason } => description.field(
            "file says",
            match reason.as_str() {
                "content-index-hit" => "not read; an earlier run answered",
                "library-answered" => "not read; the library answered",
                "content-duplicate" => "not read; the library holds these bytes",
                _ => "not read",
            },
        ),
    }
}

/// The lines describing a verdict that identified nothing, written
/// after the `file` line, from its `reason` and its `sections`.
///
/// A failed verdict carries no record to lay out and no name to
/// propose. What the operator is being asked to judge is why the file
/// got no further, and what the file's own titles say, which is what an
/// operator supplying an identifier has to go on. A skip with no
/// sections — one made after the file resolved — is shown by its reason
/// alone.
fn failure(description: &mut Description, reason: &SkipReason, sections: Option<&Sections>) {
    match reason {
        // The same slot as an identifier that was found: its question
        // is what was looked up, and the answer here is nothing.
        SkipReason::NoTextLayer => {
            description.field("identifier", "none found; the pages read hold no text");
        }
        SkipReason::TextWithoutIdentifier => {
            description.field("identifier", "none found in its metadata or the pages read");
        }
        SkipReason::Encrypted => {
            description.field("identifier", "none read; the file is encrypted");
        }
        SkipReason::Unreadable { message } => {
            description.field("unreadable", message);
        }
        // The identifier is the thing a person asked to supply a better
        // one has to improve on, so it leads — and whole, as on a
        // resolution.
        SkipReason::Unresolvable => {
            if let Some(sections) = sections {
                if let Some(looked_up) = looked_up(sections) {
                    description.whole("identifier", &looked_up);
                }
                match &sections.lookup {
                    LookupStep::Attempted { attempts, .. } => no_record(description, attempts),
                    LookupStep::NoEligibleService { .. } | LookupStep::NotAttempted { .. } => {
                        no_record(description, &[]);
                    }
                }
            }
        }
        // Both titles are already labelled on a resolution's layout, so
        // a conflict borrows those two labels and adds only how close
        // the two were.
        SkipReason::Conflict => {
            if let Some(MatchCheckStep::Conflict {
                field,
                extracted,
                resolved,
                similarity,
            }) = sections.map(|sections| &sections.match_check)
            {
                description.field("title", resolved);
                description.field("file says", extracted);
                description.field("conflict", &alike(field, *similarity));
            }
            return;
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
        _ => return,
    }
    if let Some(sections) = sections {
        file_says(description, &sections.extraction.titles);
    }
}

/// The `no record` block: what each service was asked and what it
/// said, one to a line, in the order they were asked, or that no
/// source was asked.
///
/// It replaces the `record` line rather than joining it — they answer
/// the same question, and a file has either a record or the reasons it
/// has none.
fn no_record(description: &mut Description, attempts: &[ServiceAnswer]) {
    match attempts.split_first() {
        None => description.field("no record", "no source was asked"),
        Some((first, rest)) => {
            description.field("no record", &first.said());
            for attempt in rest {
                description.field("", &attempt.said());
            }
        }
    }
}

/// A `rejected` line for each candidate `sections` says the operator
/// rejected, in submission order, each identifier written whole.
fn rejected(description: &mut Description, sections: &Sections) {
    for identifier in rejected_identifiers(sections) {
        description.whole("rejected", identifier);
    }
}

/// What became of a record an identifier the operator gave led to.
///
/// Every one of these is about a candidate rather than about the file.
/// The question put again is where the operator is told, and
/// [`reported`] is what it says; the file's verdict carries the same
/// outcome as evidence, in its `identifier_input`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidate<'a> {
    /// No service held the identifier: what each of them answered.
    Unheld { attempts: &'a [ServiceAnswer] },
    /// The name its record renders is taken by another file.
    NameTaken { target: &'a str },
    /// Its record renders no usable name.
    Unnameable,
    /// Its record names the file exactly as it is called now.
    AlreadyNamed,
}

/// The lines reporting what `identifier` came to, shown above the
/// description of the question that is put again.
///
/// `lead` labels the identifier with how the operator reached it —
/// `supplied` for one they typed, `tried again` for a lookup they had
/// repeated — since the block otherwise says nothing about which of
/// the two just happened.
///
/// A blank line closes the block, so the rule the description opens
/// with reads as the beginning of the file's own account rather than
/// as part of this one.
pub fn reported(
    lead: &str,
    identifier: &str,
    outcome: &Candidate<'_>,
    width: usize,
) -> Vec<String> {
    let mut description = Description {
        lines: Vec::new(),
        width,
    };
    // Whole, for the reason a description writes an identifier whole:
    // one folded across two lines cannot be read back.
    description.whole(lead, identifier);
    match outcome {
        Candidate::Unheld { attempts } => no_record(&mut description, attempts),
        Candidate::NameTaken { target } => description.field("name taken", target),
        Candidate::Unnameable => {
            description.field("no name", "the record is too sparse to name a file");
        }
        Candidate::AlreadyNamed => {
            description.field(
                "same name",
                "the file already carries the name this record renders",
            );
        }
    }
    description.lines.push(String::new());
    description.lines
}

/// The lines naming the work the library already holds a file for,
/// shown above the description of the question about filing this file
/// as another artifact of it.
///
/// `existing` is the artifact recorded against that work whose path
/// still holds a file, and `item` the file the work's own record was
/// read from, which is `None` only where the store can no longer name
/// one. Both are named relative to `library` where they lie under it:
/// the operator is deciding within one library, so the part of a path
/// every file shares says nothing.
///
/// A blank line closes the block, as it does after [`reported`], so the
/// rule below it reads as the beginning of the file's own account.
pub fn archived(library: &Path, existing: &Path, item: Option<&Path>, width: usize) -> Vec<String> {
    let mut description = Description {
        lines: Vec::new(),
        width,
    };
    description.field("same work", &under(library, existing));
    if let Some(item) = item {
        description.field("item", &under(library, item));
    }
    description.lines.push(String::new());
    description.lines
}

/// `path` as a description names it within `library`: relative to it
/// where it lies under it, and whole where it does not.
fn under(library: &Path, path: &Path) -> String {
    path.strip_prefix(library)
        .unwrap_or(path)
        .display()
        .to_string()
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

/// The `record` line: the services that supplied `record`
/// ([`services_of`]), and whether it was held by the library or by the
/// library at a root a content-index reference names, as `retrieval`
/// says. `None` when there is nothing to name.
///
/// A library answer whose record names no service is named by where it
/// was kept: `the library`, or `the library at <root>`.
fn record_from(record: &Record, retrieval: Option<&RetrievedFrom>) -> Option<String> {
    let named = services_of(record, retrieval)
        .into_iter()
        .map(service_name)
        .collect::<Vec<_>>()
        .join(", ");
    let kept = match retrieval {
        Some(RetrievedFrom::Library { .. }) => Some("the library".to_string()),
        Some(RetrievedFrom::LibraryReference { root, .. }) => {
            Some(format!("the library at {}", root.display()))
        }
        _ => None,
    };
    let from = match (kept, named.is_empty()) {
        (Some(kept), true) => kept,
        (Some(kept), false) => format!("{named}, from {kept}"),
        (None, _) => named,
    };
    (!from.is_empty()).then_some(from)
}

/// The `library` line, saying why the library could not answer for the
/// file, where `library` is such an answer; nothing otherwise.
fn unanswered(description: &mut Description, library: Option<&LibraryAnswer>) {
    if let Some(what) = library.and_then(why_unanswered) {
        description.field("library", &what);
    }
}

/// The lines about the content index's references to libraries, in
/// entry order: a `reference` line for each that could not answer,
/// saying why ([`why_unreachable`]), then one `references` line naming
/// what each disagreeing reference names, when any disagreed. Nothing
/// for references that answered or agreed.
fn unreferenced(description: &mut Description, sections: &Sections) {
    let references = references_read(sections);
    for why in references.iter().filter_map(why_unreachable) {
        description.field("reference", &why);
    }
    if let Some(list) = disagreement(references) {
        description.field("references", &format!("disagree: {list}"));
    }
}

/// A service as a person reads it, spelled as it spells itself: the
/// stream's lowercase name is the form a `--json` consumer matches on.
fn service_name(service: &str) -> &str {
    match service {
        "crossref" => "Crossref",
        "openalex" => "OpenAlex",
        "arxiv" => "arXiv",
        "datacite" => "DataCite",
        "pubmed" => "PubMed",
        "sidecar" => "a sidecar",
        other => other,
    }
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
