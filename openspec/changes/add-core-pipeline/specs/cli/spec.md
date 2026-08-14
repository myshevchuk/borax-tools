# cli — delta for add-core-pipeline

## ADDED Requirements

### Requirement: Single binary with subcommands
The suite SHALL ship as one binary, `borax`, with at minimum the
subcommands `resolve` (extract + resolve, emit records), `rename` (full
pipeline: resolve, plan, preview/apply), `bib` (emit/merge bibliography
output for already-resolved files), `undo` (revert the last applied run),
`config` (show effective configuration), and `cache` (inspect and clear
the response cache).

#### Scenario: Pipeline via one command
- **WHEN** `borax rename --apply <dir>` runs
- **THEN** extraction, resolution, planning, renaming, and configured
  bibliography output all occur in that single invocation

### Requirement: JSON Lines output is first-class
Every subcommand SHALL support `--json`, emitting one JSON object per line
to stdout. Each event SHALL carry an event type and a schema version;
schemas are stable within a major version of borax. Human-readable output
and JSON output SHALL be renderings of the same event stream, and
diagnostics SHALL go to stderr so stdout stays machine-parseable.

#### Scenario: Machine-readable run
- **WHEN** `borax rename --json` processes a batch
- **THEN** stdout contains only well-formed JSON Lines (per-file events
  plus a summary event) and any progress or warnings appear on stderr

### Requirement: Exit codes distinguish partial success
The binary SHALL exit 0 when every input file succeeded, exit with a
dedicated nonzero code when the run completed but one or more files were
skipped, and exit with a distinct code for fatal errors (bad
configuration, unusable arguments).

#### Scenario: Batch with skips
- **WHEN** a run completes with 8 successes and 2 skipped files
- **THEN** the exit code is the dedicated partial-success code, distinct
  from both 0 and the fatal-error code

### Requirement: Configuration resolution order
Configuration SHALL be TOML and resolve with this precedence, highest
first: command-line flags, environment variables, the nearest per-directory
override file (`.borax.toml`, discovered upward from each input file's
directory), the XDG global configuration file, built-in defaults.
`borax config` SHALL print the effective configuration with the origin of
each value.

#### Scenario: Per-directory template override
- **WHEN** a directory tree contains a `.borax.toml` defining a filename
  template different from the global configuration
- **THEN** files under that directory render with the per-directory
  template and `borax config` run there reports the override file as the
  value's origin

### Requirement: Non-interactive contexts never prompt
When stdin is not a terminal, the binary SHALL never wait for interactive
input; any operation that would prompt falls back to its safe
non-interactive behavior (preview, skip, or fail with a typed error).

#### Scenario: Piped invocation
- **WHEN** `borax rename --apply` runs with stdin redirected from
  /dev/null
- **THEN** the run completes without prompting, skipping anything that
  would have required confirmation
