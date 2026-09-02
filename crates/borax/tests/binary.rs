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

/// `borax <subcommand> --json <directory>`, run with nothing in the
/// environment that could reach the configuration beyond a
/// `XDG_CONFIG_HOME` that holds none.
fn run_json(subcommand: &str, directory: &Path, config_home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_borax"))
        .args([subcommand, "--json"])
        .arg(directory)
        .env_clear()
        .env("XDG_CONFIG_HOME", config_home)
        .env("APPDATA", config_home)
        .output()
        .expect("the binary should run")
}

/// `borax rename --json <directory>`, run with nothing in the
/// environment that could reach the configuration beyond a
/// `XDG_CONFIG_HOME` that holds none.
fn rename_json(directory: &Path, config_home: &Path) -> Output {
    run_json("rename", directory, config_home)
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

/// Spec requirement: "Tab-separated format contract" — a row with no
/// value cell "MUST be skipped and reported as a warning, without
/// aborting the load".
///
/// The warning is the whole point of the row being dropped rather than
/// refused: an abbreviation silently absent from the table is a wrong
/// name nobody can account for. So the run goes ahead, framing and all,
/// and says on stderr which line it did not use.
#[test]
fn a_dropped_row_warns_and_the_run_goes_on() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_declaring(
        home.path(),
        "abbreviation\ttitle\nAA\tAmino Acids\n\tArchives of Biochemistry\n",
    );

    let output = rename_json(home.path(), config_home.path());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stderr.contains("tables.jcode"), "got {stderr:?}");
    assert!(stderr.contains("line 3"), "got {stderr:?}");
    // The run was not refused: a dropped row is a warning, not a fault.
    assert!(stdout.contains("run-started"), "got {stdout:?}");
    assert!(stdout.contains("run-finished"), "got {stdout:?}");
}

// ---------------------------------------------------------------------
// A run compiles the template tables it renders from, and no others:
// design.md decision D6, and the spec's "Templates fail fast at load
// time" and "Citation-key templates are configured separately"
// requirements.
// ---------------------------------------------------------------------

/// A directory holding `configuration` as its `.borax.toml`, and one
/// file for the run to have something to work on.
fn library_with(directory: &Path, configuration: &str) {
    fs::write(directory.join(".borax.toml"), configuration).expect("the config should be written");
    fs::write(directory.join("paper.pdf"), b"not really a PDF")
        .expect("the file should be written");
}

/// A `.borax.toml` whose `templates.default` will not compile: an
/// unclosed `[` at byte 11 of `[auth][year`. `citation-keys` is left
/// unset, so it falls back to the built-in default, which does compile.
const BROKEN_FILENAME_TEMPLATE: &str = r#"
[templates]
default = '[auth][year'
"#;

/// A `.borax.toml` whose `citation-keys.default` will not compile, by
/// the same broken source. `templates` is left unset, so it falls back
/// to the built-in default, which does compile.
const BROKEN_CITATION_KEY_TEMPLATE: &str = r#"
[citation-keys]
default = '[auth][year'
"#;

/// Spec scenario: "A filename template stops only the runs that render
/// one" — `borax bib` renders no filename, so an uncompilable
/// `templates.default` must not end it.
///
/// Red today: `compiled_groups` still compiles both template tables for
/// `bib`, so this run is refused before either framing event.
#[test]
fn an_uncompilable_filename_template_lets_bib_run_to_completion() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_with(directory.path(), BROKEN_FILENAME_TEMPLATE);

    let output = run_json("bib", directory.path(), config_home.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Not pinned to the partial-success code: that digit is about the
    // fixture being nine bytes of ASCII rather than a PDF, not about
    // this change.
    assert_ne!(output.status.code(), Some(1), "stderr was {stderr:?}");
    assert!(stdout.contains("run-started"), "got {stdout:?}");
    assert!(stdout.contains("run-finished"), "got {stdout:?}");
}

/// The other half of the same scenario: `rename` renders a filename
/// from that same template, so the same configuration must still end
/// it with the fatal code and neither framing event.
///
/// Green today, and it is the regression guard that says the scoping
/// cut in one direction only.
#[test]
fn an_uncompilable_filename_template_still_ends_rename() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_with(directory.path(), BROKEN_FILENAME_TEMPLATE);

    let output = run_json("rename", directory.path(), config_home.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1), "stderr was {stderr:?}");
    assert!(stderr.contains("templates.default"), "got {stderr:?}");
    assert!(!stdout.contains("run-started"), "got {stdout:?}");
    assert!(!stdout.contains("run-finished"), "got {stdout:?}");
}

/// Spec scenario (citation-keys spec): "Uncompilable citation-key
/// template" — `bib` renders citation keys, so this one it must still
/// refuse.
///
/// Green today; it guards against stage 3 scoping too much away.
#[test]
fn an_uncompilable_citation_key_template_still_ends_bib() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_with(directory.path(), BROKEN_CITATION_KEY_TEMPLATE);

    let output = run_json("bib", directory.path(), config_home.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1), "stderr was {stderr:?}");
    assert!(stderr.contains("citation-keys.default"), "got {stderr:?}");
    assert!(!stdout.contains("run-started"), "got {stdout:?}");
    assert!(!stdout.contains("run-finished"), "got {stdout:?}");
}

/// Spec requirement: "Table failures end a run before it starts" holds
/// for both commands — table loading is not scoped along with the
/// template tables the way the templates themselves now are.
///
/// The `rename` half is green today; the `bib` half must stay green
/// through stage 3, which is exactly the point of writing it.
#[test]
fn a_table_that_cannot_be_read_ends_both_rename_and_bib() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_declaring(directory.path(), "title\tabbreviation\n");
    let table = directory.path().join("journals.tsv");
    fs::remove_file(&table).expect("the table should be removable");
    let reason = table.display().to_string();

    let rename_output = run_json("rename", directory.path(), config_home.path());
    let bib_output = run_json("bib", directory.path(), config_home.path());

    assert_refused(&rename_output, &reason);
    assert_refused(&bib_output, &reason);
}

/// Spec: `design.md` decision D6 — "`borax config` compiles nothing,
/// and this change does not make it start." A `templates.default` that
/// will not compile is still reported as the source text a
/// configuration file gave, with its origin, and the run is not
/// refused.
///
/// Green today; pins that this change does not make `config` start
/// validating.
#[test]
fn config_reports_an_uncompilable_template_as_its_source_text() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let config_home = tempfile::tempdir().expect("a temporary config home");
    library_with(directory.path(), BROKEN_FILENAME_TEMPLATE);

    // `config` takes no path argument, so the fixture directory has to
    // be the working directory for the override search to find it.
    let output = Command::new(env!("CARGO_BIN_EXE_borax"))
        .arg("config")
        .current_dir(directory.path())
        .env_clear()
        .env("XDG_CONFIG_HOME", config_home.path())
        .env("APPDATA", config_home.path())
        .output()
        .expect("the binary should run");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(0), "got {stdout:?}");
    // The origin's absolute path is not pinned: `config` finds the file
    // from the working directory, which macOS reports canonicalized,
    // and the claim here is about the value and the layer it came from.
    assert!(
        stdout.contains("templates.default = \"[auth][year\"  # override "),
        "got {stdout:?}"
    );
    assert!(stdout.contains(".borax.toml"), "got {stdout:?}");
}
