//! One invocation's relationship with the shell that started it: what
//! it is allowed to ask, and what it reports back when it ends.
//!
//! Both halves exist so a run behaves the same whether a person or a
//! script started it. A script needs an exit code it can branch on, and
//! it needs the certainty that no invocation will ever sit waiting for
//! an answer nobody is there to give.
//!
//! The two functions that touch the process itself — reading whether
//! stdin is a terminal, and handing a code to the operating system —
//! are the adapter; everything a test cares about is decided by the
//! pure functions above them.

use std::io::{self, IsTerminal};

use crate::event::{Counts, Format};

/// How an invocation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The run completed and left nothing behind: no file was skipped.
    Success,
    /// The run completed, but at least one file was skipped.
    Partial,
    /// The run did not happen. The configuration or the arguments were
    /// unusable, so nothing was attempted and no file was touched.
    Fatal,
}

/// The exit code of a run that completed with nothing skipped.
pub const SUCCESS: i32 = 0;

/// The exit code of a run that never started.
///
/// The code a shell treats as ordinary failure, because an invocation
/// that could not be understood is the ordinary failure.
pub const FATAL: i32 = 1;

/// The exit code of a completed run that skipped at least one file.
///
/// Distinct from both [`SUCCESS`] and [`FATAL`]: `&&` still treats a run
/// with skips as a failure, while a script that wants to tell "some
/// files need attention" from "borax could not run" reads the two codes
/// apart.
pub const PARTIAL: i32 = 2;

impl Outcome {
    /// The process exit code this outcome leaves behind.
    pub fn code(self) -> i32 {
        todo!("map each outcome to its code")
    }
}

/// How a run that completed ended, from its totals.
///
/// Only [`Counts::skipped`] decides it: a run over an empty directory
/// resolves nothing, skips nothing, and succeeds, because there was
/// nothing it failed to do. [`Outcome::Fatal`] is not reachable from
/// totals — a run that produced totals is a run that happened.
pub fn outcome_for(counts: &Counts) -> Outcome {
    todo!("partial when anything was skipped")
}

/// Whether this invocation may ask the person running it a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    /// A question may be put, and an answer waited for.
    Allowed,
    /// Nothing may be asked. Every choice that would have been a
    /// question takes its safe answer instead.
    Forbidden,
}

/// Whether `format` and a terminal on stdin leave room to ask.
///
/// Asking requires both: a terminal, because there is otherwise nobody
/// to answer, and [`Format::Human`], because `--json` is how a caller
/// says a program is driving the run. Either one absent forbids the
/// question rather than deferring it.
pub fn interaction(stdin_is_terminal: bool, format: Format) -> Interaction {
    todo!("allow only a human-format run on a terminal")
}

/// The answer to a yes/no question under `interaction`.
///
/// `ask` is called only when asking is [`Interaction::Allowed`]; when it
/// is [`Interaction::Forbidden`] the question is not put at all and the
/// answer is `false`.
///
/// An unasked confirmation is never granted, so the caller's `false`
/// branch has to be the one that leaves the file alone — that is what
/// makes a piped run fall back to skipping rather than to acting.
pub fn confirm(interaction: Interaction, ask: impl FnOnce() -> bool) -> bool {
    todo!("ask only when allowed")
}

/// Whether this process's standard input is a terminal.
pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}
