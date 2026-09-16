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

use crate::event::Event;

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

/// The lines shown above a question about `resolved`, proposing
/// `proposal`.
///
/// `resolved` is the file's [`Event::Resolved`]; any other event has no
/// description and yields no lines.
///
/// The lines, in order, leaving out every field the record does not
/// hold: a rule carrying `position`; the file's name; the identifier
/// the run looked up and where it was found; the services that
/// supplied the record; its type, title, authors, date of issue and
/// container; the titles the file claims for itself with where each was
/// read; and the name the file would take, with a line saying which
/// rendered name was taken when a suffix moved it aside.
///
/// Values wrap at `width` under a hanging indent. Nothing is truncated
/// except the list of authors, which gives three and a count: a title
/// cut short is what would hide the difference between two works.
pub fn describe(
    resolved: &Event,
    proposal: &Proposal,
    position: Position,
    width: usize,
) -> Vec<String> {
    let _ = (resolved, proposal, position, width);
    todo!("describe: the evidence a question rests on, wrapped to width")
}
