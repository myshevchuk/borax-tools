# run-logs — delta for add-ledger-and-run-logs

## ADDED Requirements

### Requirement: Runs persist their event stream as JSONL run logs
Each run SHALL be able to persist its complete typed event stream — the
same versioned JSON Lines schema that `--json` prints — to a run-log
file named `<UTC-timestamp>-<command>-<dry|apply>.jsonl` under
`.borax/runs/` at the collection root. No other format SHALL be used
for run records.

#### Scenario: Dry and apply runs pair in a listing
- **WHEN** a preview run is followed by its apply run
- **THEN** the runs directory contains two files whose names sort
  adjacently, differing in the `dry`/`apply` suffix and timestamp

#### Scenario: Run log equals --json output
- **WHEN** a run executes with `--json` and run-logging enabled
- **THEN** the persisted run log contains the same events as the
  emitted stdout stream

### Requirement: Apply-run logs are mandatory and flushed before mutation
An `--apply` run SHALL create its run log and flush the planned rename
events to disk before executing the first rename; if the log cannot be
created or written, the run SHALL abort before mutating anything.
Apply-run logging cannot be disabled.

#### Scenario: Run-log directory unwritable
- **WHEN** an apply run cannot create its run log
- **THEN** the run aborts with a clear error and every file keeps its
  original name

#### Scenario: --no-run-log on an apply run
- **WHEN** `--no-run-log` is passed together with `--apply`
- **THEN** the dry-run-log suppression does not apply: the apply-run
  log is still written

### Requirement: Dry-run logs are optional and best-effort
Preview runs SHALL write run logs by default; `--no-run-log` (or its
config key) disables them, and a failure to write one warns without
failing the run.

#### Scenario: Suppressed dry-run log
- **WHEN** a preview run executes with `--no-run-log`
- **THEN** no run-log file is created and the run completes normally

### Requirement: Apply-run logs fall back to XDG state outside a collection
An apply run on files outside any collection root SHALL write its run
log under the XDG state directory instead, in a location `borax undo`
can discover, so undo works everywhere.

#### Scenario: Apply in a downloads directory
- **WHEN** an apply run renames files in a directory with no
  `.borax.toml` above it
- **THEN** the run log is written under the XDG state directory and a
  subsequent `borax undo` reverts the run
