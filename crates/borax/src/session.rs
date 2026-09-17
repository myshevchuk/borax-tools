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
use std::path::PathBuf;

use crate::describe::DEFAULT_WIDTH;
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
    /// Carry it out although the record's title and the file's own
    /// disagree. Offered only where they do, never the default, and
    /// never given by anything but a person.
    Override,
    /// Leave the file with the name it has, which is already the name
    /// its record implies. Offered where there was no move to make.
    Keep,
    /// Say what the file is, and decide again from the record that
    /// identifier resolves to.
    Supply,
    /// Try the same lookup again, where what failed was the asking
    /// rather than the answer.
    Retry,
    /// Leave the file as it is and go on to the next.
    Skip,
    /// End the run here, leaving this file and every file after it
    /// untouched.
    Quit,
}

/// A request for text rather than a choice: what is being asked for,
/// and what to say about the last answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPrompt {
    /// What is wanted, as one line.
    pub asking: String,
    /// What was wrong with the last attempt, where there was one. Shown
    /// above the prompt so the operator reads it before typing again.
    pub refused: Option<String>,
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

    /// Ask for an identifier, and return what was typed.
    ///
    /// `None` is the operator declining to answer — an empty line, or
    /// an escape — which leaves the file exactly as the question found
    /// it. What comes back is text and not an identifier: the run
    /// parses it, and says so again when it is not one.
    fn text(&mut self, prompt: &TextPrompt) -> Option<String>;
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
    /// The columns a question's description is rendered to.
    ///
    /// Part of the session rather than read where the description is
    /// built, because asking a terminal how wide it is touches the
    /// process: the run stays a pure function of what it was handed,
    /// and a session built without one is rendered at
    /// [`DEFAULT_WIDTH`].
    pub width: usize,
    /// The directory a question names its file relative to: the one the
    /// run was started in.
    ///
    /// Here for the same reason as `width`: it is read from the
    /// invocation, and a run given no directory names its files whole.
    pub working: PathBuf,
}

impl<'a> Session<'a> {
    /// A run that asks nothing.
    pub fn batch() -> Session<'a> {
        Session {
            mode: Mode::Batch,
            asker: None,
            width: DEFAULT_WIDTH,
            working: PathBuf::new(),
        }
    }

    /// A run that puts its decisions to `asker`, rendered to the
    /// default width.
    pub fn interactive(asker: &'a mut dyn Asker) -> Session<'a> {
        Session {
            mode: Mode::Interactive,
            asker: Some(asker),
            width: DEFAULT_WIDTH,
            working: PathBuf::new(),
        }
    }

    /// The same session, rendering its questions to `width` columns.
    pub fn at_width(self, width: usize) -> Session<'a> {
        Session { width, ..self }
    }

    /// The same session, naming the files it asks about relative to
    /// `working`.
    pub fn started_in(self, working: PathBuf) -> Session<'a> {
        Session { working, ..self }
    }
}

/// The terminal's width in columns, or [`DEFAULT_WIDTH`] where there is
/// no terminal to ask or it does not say.
///
/// A width of nothing is treated as no answer: a description wrapped to
/// zero columns is one word a line.
pub fn terminal_width() -> usize {
    match crossterm::terminal::size() {
        Ok((columns, _)) if columns > 0 => usize::from(columns),
        _ => DEFAULT_WIDTH,
    }
}

/// Whether this process's standard input is a terminal.
pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}

/// One menu entry, as [`inquire`] needs it: an answer and the words
/// the operator reads for it.
struct Choice {
    answer: Answer,
    /// The name the file would take, for the one choice that says what
    /// it will do rather than what it is.
    target: String,
}

impl fmt::Display for Choice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.answer {
            Answer::Rename => formatter.write_str("Rename"),
            // The one choice that overrides a safety check names the
            // target it would move the file to, so that the answer
            // says exactly what it will do and the operator is not
            // reading it against a description they may have scrolled
            // past.
            //
            // Escaped for the reason the description is: a target is
            // rendered from a record somebody else wrote, and
            // `sanitize` replaces only the control characters below
            // U+0020 — a C1 control such as U+009B survives into a
            // filename, and this is the label of the one choice that
            // overrides a check.
            Answer::Override => {
                write!(
                    formatter,
                    "Rename anyway, to {}",
                    crate::describe::escaped(&self.target)
                )
            }
            Answer::Keep => formatter.write_str("Keep this name"),
            Answer::Supply => formatter.write_str("Supply an identifier"),
            Answer::Retry => formatter.write_str("Try the services again"),
            Answer::Skip => formatter.write_str("Skip"),
            Answer::Quit => formatter.write_str("Quit"),
        }
    }
}

/// The name a question's target carries, as its menu names it:
/// relative to the directory the file sits in, so a template filing it
/// elsewhere keeps the subdirectory that says where it goes.
fn target_of(question: &Question) -> String {
    match question.path.parent() {
        Some(parent) => crate::paths::route(&question.target, parent)
            .display()
            .to_string(),
        None => question.target.display().to_string(),
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
pub struct TerminalAsker;

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
        // Above the menu rather than in its message, because it
        // is a dozen lines and a prompt is one line. A terminal
        // that will not take them is a terminal the question
        // cannot be put on, so it ends the session rather than
        // ending the process: `eprintln!` would panic on the write
        // that failed.
        if writeln!(io::stderr(), "{}", question.description.join("\n")).is_err() {
            return Answer::Quit;
        }

        let target = target_of(question);
        let choices: Vec<Choice> = question
            .choices
            .iter()
            .map(|answer| Choice {
                answer: *answer,
                target: target.clone(),
            })
            .collect();
        // Without a help message, `inquire` offers its own, which
        // advertises filtering by typing. Four choices do not need
        // filtering, and the offer reads as though an answer could be
        // typed.
        //
        // The prompt asks what should happen rather than whether to
        // rename: the questions a run puts are about files it could
        // not identify as much as about moves it proposes.
        match inquire::Select::new("What should happen to this file?", choices)
            .with_help_message("↑↓ to move, enter to select")
            .prompt()
        {
            Ok(choice) => choice.answer,
            Err(_) => Answer::Quit,
        }
    }

    /// Ask for a line of text, and return it with its edges trimmed.
    ///
    /// An empty line and an interrupted prompt are both `None`: the
    /// operator declining to answer leaves the file as the question
    /// found it, which is what an unanswered question has always
    /// meant. Esc, Ctrl-C and a terminal that cannot be read at all
    /// are the interruptions [`inquire`] reports as errors, and they
    /// are all the same answer here.
    ///
    /// What was wrong with the last attempt is written above the
    /// prompt rather than put in it, as a question's description is
    /// and for the same reason: a prompt is one line, and the operator
    /// reads the refusal before typing again.
    fn text(&mut self, prompt: &TextPrompt) -> Option<String> {
        if let Some(refused) = &prompt.refused {
            // A terminal that will not take the refusal is a terminal
            // the question cannot be put on, so the prompt is
            // abandoned rather than put unexplained.
            writeln!(io::stderr(), "{refused}").ok()?;
        }

        let typed = inquire::Text::new(&prompt.asking)
            .with_help_message("enter to look it up, esc to go back")
            .prompt()
            .ok()?;
        match typed.trim() {
            "" => None,
            trimmed => Some(trimmed.to_string()),
        }
    }
}
