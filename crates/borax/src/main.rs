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
use clap::Parser;

fn main() -> ExitCode {
    let cli = Cli::parse();
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
