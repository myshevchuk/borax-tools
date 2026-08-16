//! The `borax` binary: a shell over the library and nothing else.
//!
//! Everything an invocation does lives in [`borax`], so an integration
//! test runs a whole one without spawning a process. What is here is
//! the two things a test cannot have: the real streams, and an exit
//! code handed back to the shell.

use std::io;
use std::process::ExitCode;

use borax::cli::Cli;
use borax::run::{Streams, execute};
use borax::session::{FATAL, SUCCESS};
use clap::Parser;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return parse_failure(&error),
    };

    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();

    ExitCode::from(
        execute(
            &cli,
            &mut Streams {
                out: &mut out,
                err: &mut err,
            },
        )
        .code(),
    )
}

/// Print `error` and end the process.
///
/// `--help` and `--version` arrive here too — clap reports them as
/// errors because they end the run early — and they succeed. What tells
/// them apart is which stream clap wants: it asks for stderr only when
/// something was actually wrong.
///
/// A usage error exits [`FATAL`] rather than taking clap's own code,
/// which is 2 — the code borax uses for a run that completed with files
/// skipped. An invocation that never started must not report that.
fn parse_failure(error: &clap::Error) -> ExitCode {
    let _ = error.print();
    match error.use_stderr() {
        true => ExitCode::from(FATAL),
        false => ExitCode::from(SUCCESS),
    }
}
