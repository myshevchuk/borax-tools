//! One invocation's relationship with the shell that started it: what
//! it may ask, how it asks it, and what it reports back when it ends.
//!
//! All three exist so a run behaves the same whether a person or a
//! script started it. A script needs an exit code it can branch on, and
//! it needs the certainty that no invocation will ever sit waiting for
//! an answer nobody is there to give: a run with no terminal on stdin,
//! or one rendering JSON, is a batch run whatever else was asked for.
//!
//! The two functions that touch the process itself — reading whether
//! stdin is a terminal, and handing a code to the operating system —
//! are the adapter; everything a test cares about is decided by the
//! pure functions above them.

use std::io::{self, IsTerminal};
use std::path::PathBuf;

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
pub const SUCCESS: u8 = 0;

/// The exit code of a run that never started.
///
/// The code a shell treats as ordinary failure, because an invocation
/// that could not be understood is the ordinary failure.
pub const FATAL: u8 = 1;

/// The exit code of a completed run that skipped at least one file.
///
/// Distinct from both [`SUCCESS`] and [`FATAL`]: `&&` still treats a run
/// with skips as a failure, while a script that wants to tell "some
/// files need attention" from "borax could not run" reads the two codes
/// apart.
pub const PARTIAL: u8 = 2;

impl Outcome {
    /// The process exit code this outcome leaves behind.
    pub fn code(self) -> u8 {
        match self {
            Outcome::Success => SUCCESS,
            Outcome::Partial => PARTIAL,
            Outcome::Fatal => FATAL,
        }
    }
}

/// How a run that completed ended, from its totals.
///
/// Only [`Counts::skipped`] decides it: a run over an empty directory
/// resolves nothing, skips nothing, and succeeds, because there was
/// nothing it failed to do. [`Outcome::Fatal`] is not reachable from
/// totals — a run that produced totals is a run that happened.
pub fn outcome_for(counts: &Counts) -> Outcome {
    match counts.skipped {
        0 => Outcome::Success,
        _ => Outcome::Partial,
    }
}

/// Whether a run puts its decisions to the person who started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Every decision is put as a question and waited for.
    Interactive,
    /// Nothing is asked. The run reports what it would do, or carries
    /// out what `--apply` authorised.
    Batch,
}

/// The mode a rename run takes.
///
/// Interactive requires all four: a terminal on stdin, because there is
/// otherwise nobody to answer; [`Format::Human`], because `--json` is
/// how a caller says a program is driving the run; no `--apply`, which
/// authorises a plan and therefore asks for the run that carries one
/// out; and `batch` off, which is its default.
///
/// `batch` is the run's own setting rather than any input directory's:
/// a run is one session with one operator, and the mode is settled here
/// once, before the first event.
///
/// Every other command is [`Mode::Batch`]: only `rename` has a decision
/// to put to anyone.
pub fn mode(stdin_is_terminal: bool, format: Format, batch: bool, apply: bool) -> Mode {
    let _ = (stdin_is_terminal, format, batch, apply);
    todo!("mode: interactive only on a human terminal with no --apply and batch off")
}

/// What an interactive run does about one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The file the question is about.
    pub path: PathBuf,
    /// Where it would move to, as the operator is shown it: the name
    /// the file would take, including any collision suffix.
    pub target: PathBuf,
    /// What may be answered, in the order they are offered. The first
    /// is the default, so it is never the answer that moves a file
    /// against a doubt.
    pub choices: Vec<Answer>,
}

/// One answer to a [`Question`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Carry out the move the question named.
    Rename,
    /// Leave the file as it is and go on to the next.
    Skip,
    /// End the run here, leaving this file and every file after it
    /// untouched.
    Quit,
}

/// Where an interactive run's questions are put and answered.
///
/// The seam between the run and the terminal. A run is written against
/// this and never against a terminal, so every path through it is
/// tested with answers supplied from a list.
pub trait Asker {
    /// Put `question` and return the answer.
    ///
    /// The answer is one of `question.choices`. An implementation that
    /// cannot ask — the operator interrupted, the terminal went away —
    /// answers [`Answer::Quit`], which is the answer that touches
    /// nothing further.
    fn choose(&mut self, question: &Question) -> Answer;
}

/// Whether this process's standard input is a terminal.
pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}
