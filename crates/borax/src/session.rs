//! One invocation's relationship with the shell that started it: what
//! it may ask, how it asks it, and what it reports back when it ends.
//!
//! All three exist so a run behaves the same whether a person or a
//! script started it. A script needs an exit code it can branch on, and
//! it needs the certainty that no invocation will ever sit waiting for
//! an answer nobody is there to give: a run with no terminal on stdin,
//! or one rendering JSON, is a batch run whatever else was asked for.
//!
//! What touches the process itself — reading whether stdin is a
//! terminal, putting a question to the person at it, and handing a code
//! to the operating system — is the adapter; everything a test cares
//! about is decided by the pure functions above it, and a run is
//! written against [`Asker`] rather than against a terminal.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

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
/// [`Counts::skipped`] and [`Counts::unreached`] decide it, and each
/// means a file the run did not finish with: one it looked at and left,
/// and one it never reached because the run ended early. A run over an
/// empty directory resolves nothing, skips nothing, reaches the end of
/// its inputs, and succeeds, because there was nothing it failed to do.
/// [`Outcome::Fatal`] is not reachable from totals — a run that
/// produced totals is a run that happened.
pub fn outcome_for(counts: &Counts) -> Outcome {
    match counts.skipped + counts.unreached {
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
    match stdin_is_terminal && format == Format::Human && !batch && !apply {
        true => Mode::Interactive,
        false => Mode::Batch,
    }
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
    /// What the operator is shown before the question: the evidence the
    /// answer rests on, already rendered
    /// ([`crate::describe::describe`]). Empty for a question that
    /// carries none.
    ///
    /// Rendered by the run rather than by the asker, so what a person
    /// is shown is decided where every other rendering decision is, and
    /// the adapter that draws it stays a translation.
    pub description: Vec<String>,
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

/// One invocation's relationship with its operator: which mode it runs
/// in, and where its questions go.
///
/// Carried down through [`crate::run::dispatch`] rather than held in
/// the run's adapters, because asking is part of the invocation and not
/// part of the environment it reads: a batch run has no operator, and
/// says so by carrying no asker at all.
pub struct Session<'a> {
    pub mode: Mode,
    /// Where questions go. `None` in a batch run, which asks none, so a
    /// driver that tries to ask one has nowhere to send it rather than
    /// a default answer to invent.
    pub asker: Option<&'a mut dyn Asker>,
}

impl<'a> Session<'a> {
    /// A run that asks nothing.
    pub fn batch() -> Session<'a> {
        Session {
            mode: Mode::Batch,
            asker: None,
        }
    }

    /// A run that puts its decisions to `asker`.
    pub fn interactive(asker: &'a mut dyn Asker) -> Session<'a> {
        Session {
            mode: Mode::Interactive,
            asker: Some(asker),
        }
    }
}

/// Whether this process's standard input is a terminal.
pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}

/// One menu entry, as [`inquire`] needs it: an answer and the word the
/// operator reads for it.
struct Choice(Answer);

impl fmt::Display for Choice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            Answer::Rename => "Rename",
            Answer::Skip => "Skip",
            Answer::Quit => "Quit",
        })
    }
}

/// The [`Asker`] that puts a question to the terminal: an arrow-key
/// menu, drawn by [`inquire`].
///
/// A menu rather than a line of letter keys, because later changes add
/// choices to it and a menu stays legible as they arrive. It draws on
/// standard error, so standard output carries the event stream and
/// nothing else in an interactive run as in a batch one.
///
/// This is the whole of what touches the terminal, and it is kept to
/// translating a [`Question`] into a menu and the menu's result back
/// into an [`Answer`]: what the test suite cannot reach is therefore
/// also what holds no decision.
pub struct TerminalAsker {
    /// The directory a question's file is named relative to, where it
    /// lies under one.
    working: PathBuf,
}

impl TerminalAsker {
    /// An asker naming files relative to `working`, which is the
    /// directory the run was started from.
    pub fn new(working: PathBuf) -> TerminalAsker {
        TerminalAsker { working }
    }
}

impl Asker for TerminalAsker {
    /// Draw `question` as a menu of its choices and return the one
    /// picked, the first being where the cursor starts.
    ///
    /// Every way the menu can end without an answer is
    /// [`Answer::Quit`]: Esc and Ctrl-C, which [`inquire`] reports as
    /// errors rather than as a signal, and a terminal that cannot be
    /// read at all. Quit is the answer that touches nothing, so an
    /// interrupted question leaves the file exactly as an unanswered
    /// one does.
    fn choose(&mut self, question: &Question) -> Answer {
        // Above the menu rather than in its message, so the file and
        // the name it would take keep a line each. A terminal that will
        // not take them is a terminal the question cannot be put on, so
        // it ends the session rather than ending the process: `eprintln!`
        // would panic on the write that failed.
        if writeln!(
            io::stderr(),
            "{}\n  → {}",
            shown(&question.path, &self.working),
            target_shown(&question.target, &question.path)
        )
        .is_err()
        {
            return Answer::Quit;
        }

        let choices: Vec<Choice> = question.choices.iter().copied().map(Choice).collect();
        // Without a help message, `inquire` offers its own, which
        // advertises filtering by typing. Three choices do not need
        // filtering, and the offer reads as though an answer could be
        // typed.
        match inquire::Select::new("Rename this file?", choices)
            .with_help_message("↑↓ to move, enter to select")
            .prompt()
        {
            Ok(choice) => choice.0,
            Err(_) => Answer::Quit,
        }
    }
}

/// `path` as a question names it, from a run started in `working`:
/// relative to that directory where it lies under it, and whole where
/// it does not.
///
/// A file name alone would be shorter and is what a run over one
/// directory would show either way, but it stops identifying a file as
/// soon as a run spans two: `tree-a/paper.pdf` and `tree-b/paper.pdf`
/// are different moves, and a question that named both `paper.pdf`
/// would take one answer for the other.
fn shown(path: &Path, working: &Path) -> String {
    path.strip_prefix(working)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// `target` as a question names it, beside the file at `path`: the name
/// it would take, with the subdirectory a template sends it to where
/// there is one.
fn target_shown(target: &Path, path: &Path) -> String {
    match path
        .parent()
        .and_then(|parent| target.strip_prefix(parent).ok())
    {
        Some(relative) => relative.display().to_string(),
        None => target.display().to_string(),
    }
}
