//! The process boundary: what the binary tells the shell.
//!
//! Everything else about an invocation is tested in process against the
//! library. What cannot be is the exit code itself, so these tests are
//! the one place that spawns `borax`.

use std::process::Command;

/// `borax` with `args`, run with an environment holding nothing that
/// could reach the configuration.
///
/// Returns the exit code, or `None` for a process ended by a signal.
fn exit_code(args: &[&str]) -> Option<i32> {
    Command::new(env!("CARGO_BIN_EXE_borax"))
        .args(args)
        .env_clear()
        .status()
        .expect("the binary should run")
        .code()
}

#[test]
fn a_run_with_nothing_skipped_exits_with_the_success_code() {
    assert_eq!(exit_code(&["config"]), Some(0));
}

#[test]
fn help_exits_with_the_success_code() {
    assert_eq!(exit_code(&["--help"]), Some(0));
}

#[test]
fn version_exits_with_the_success_code() {
    assert_eq!(exit_code(&["--version"]), Some(0));
}

// The cli spec requires the fatal code to be distinct from the
// partial-success one. clap's own default for a usage error is 2, which
// is borax's partial-success code, so an unusable argument would
// otherwise report a run that half worked.
#[test]
fn an_unknown_flag_exits_with_the_fatal_code_not_the_partial_one() {
    assert_eq!(exit_code(&["--badflag"]), Some(1));
}

#[test]
fn an_unknown_subcommand_exits_with_the_fatal_code() {
    assert_eq!(exit_code(&["nonesuch"]), Some(1));
}

#[test]
fn a_missing_required_path_exits_with_the_fatal_code() {
    assert_eq!(exit_code(&["resolve"]), Some(1));
}

#[test]
fn a_configuration_that_will_not_resolve_exits_with_the_fatal_code() {
    let code = Command::new(env!("CARGO_BIN_EXE_borax"))
        .arg("config")
        .env_clear()
        .env("BORAX_NETWORK_CONCURRENCY", "not a number")
        .status()
        .expect("the binary should run")
        .code();

    assert_eq!(code, Some(1));
}
