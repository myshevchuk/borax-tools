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

// ---------------------------------------------------------------------
// External tables: every failure that is a property of a declaration or
// of the file it names ends the run before the stream opens.
//
// Spec requirement: "Table failures end a run before it starts". The
// exit code is why these are here rather than in `dispatch.rs`, and the
// stream is checked alongside it because the two claims are one
// promise: a refused run costs nothing and says why.
// ---------------------------------------------------------------------

use std::fs;
use std::path::Path;
use std::process::Output;

/// A `.borax.toml` declaring a `jcode` table over `journals.tsv` beside
/// it, with a filename template that looks it up.
const CONFIGURATION: &str = r#"
[templates]
default = '[auth][year]-[journal:lookup("jcode")]'

[tables.jcode]
path = "journals.tsv"
key = "title"
value = "abbreviation"
"#;

/// A directory holding that configuration, a `journals.tsv` of `table`,
/// and one file for the run to have something to work on.
fn library_declaring(directory: &Path, table: &str) {
    fs::write(directory.join(".borax.toml"), CONFIGURATION).expect("the config should be written");
    fs::write(directory.join("journals.tsv"), table).expect("the table should be written");
    fs::write(directory.join("paper.pdf"), b"not really a PDF")
        .expect("the file should be written");
}

/// `borax rename --json <directory>`, run with nothing in the
/// environment that could reach the configuration beyond a
/// `XDG_CONFIG_HOME` that holds none.
fn rename_json(directory: &Path, config_home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_borax"))
        .args(["rename", "--json"])
        .arg(directory)
        .env_clear()
        .env("XDG_CONFIG_HOME", config_home)
        .env("APPDATA", config_home)
        .output()
        .expect("the binary should run")
}

/// Assert that `output` is the fatal outcome a broken table produces:
/// the fatal code, `reason` on stderr, and a stdout carrying neither
/// framing event, since nothing was attempted.
fn assert_refused(output: &Output, reason: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(1), "stderr was {stderr:?}");
    assert!(stderr.contains("tables.jcode"), "got {stderr:?}");
    assert!(stderr.contains(reason), "got {stderr:?}");
    assert!(!stdout.contains("run-started"), "got {stdout:?}");
    assert!(!stdout.contains("run-finished"), "got {stdout:?}");
}

/// Spec scenario: "Declared table file is missing". The path named is
/// the one beside the `.borax.toml` that declared it, which is also
/// what pins the relative path being resolved against the declaring
/// file rather than against the working directory.
#[test]
fn a_declared_table_file_that_is_not_there_ends_the_run_before_the_stream_opens() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_declaring(directory.path(), "title\tabbreviation\n");
    let table = directory.path().join("journals.tsv");
    fs::remove_file(&table).expect("the table should be removable");

    let output = rename_json(directory.path(), config_home.path());

    assert_refused(&output, &table.display().to_string());
}

/// Spec scenario: "Declared column absent from the header".
#[test]
fn a_header_without_the_declared_value_column_ends_the_run_before_the_stream_opens() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_declaring(
        directory.path(),
        "title\tshorttitle\nAmino Acids\tAmino Acids\n",
    );

    let output = rename_json(directory.path(), config_home.path());

    assert_refused(&output, "abbreviation");
}

/// Spec scenario: "Two rows disagree about one journal".
#[test]
fn two_rows_claiming_one_key_with_different_values_end_the_run_before_the_stream_opens() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_declaring(
        directory.path(),
        "title\tabbreviation\nJ. Chem. Soc.\tJCS\nJ Chem Soc\tJCHS\n",
    );

    let output = rename_json(directory.path(), config_home.path());

    assert_refused(&output, "JCHS");
}
