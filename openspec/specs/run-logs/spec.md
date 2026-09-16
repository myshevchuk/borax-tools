# run-logs Specification

## Purpose
TBD - created by archiving change add-ledger-and-run-logs. Update Purpose after archive.
## Requirements
### Requirement: Runs persist their event stream as JSONL run logs
Each run SHALL be able to persist its complete typed event stream — the
same versioned JSON Lines schema that `--json` prints — to a run-log
file named `<UTC-timestamp>-<command>-<dry|apply>.jsonl` under
`.borax/runs/` at the collection root. No other format SHALL be used
for run records.

A move the filesystem refuses is the one event the log carries and the
stream does not. Its event is written before the move is attempted,
because that is what makes the move accountable; the stream reports
what happened, and what happened is the skip that follows. Every other
event appears in both.

#### Scenario: Dry and apply runs pair in a listing
- **WHEN** a preview run is followed by its apply run
- **THEN** the runs directory contains two files whose names sort
  adjacently, differing in the `dry`/`apply` suffix and timestamp

#### Scenario: Run log equals --json output
- **WHEN** a run executes with `--json` and run-logging enabled
- **THEN** the persisted run log contains the same events as the
  emitted stdout stream

#### Scenario: A refused move is in the log alone
- **WHEN** a move whose event was written to the log is then refused by
  the filesystem
- **THEN** the log carries that move's event followed by the skip, the
  stream carries the skip alone, and the run's totals count the file as
  skipped

### Requirement: Apply-run logs are mandatory and flushed before mutation
A run that may move files SHALL create its run log before it moves
anything, and SHALL write and flush each move's rename event before
making that move; if the log cannot be created, or an event cannot be
written, the run SHALL abort before mutating anything further.
Apply-run logging cannot be disabled.

A run that may move files is an `--apply` run or an interactive rename
run, whether or not it turns out to move anything.

#### Scenario: Run-log directory unwritable
- **WHEN** an apply run cannot create its run log
- **THEN** the run aborts with a clear error and every file keeps its
  original name

#### Scenario: --no-run-log on an apply run
- **WHEN** `--no-run-log` is passed together with `--apply`
- **THEN** the dry-run-log suppression does not apply: the apply-run
  log is still written

#### Scenario: The log stops taking writes midway
- **WHEN** a run's log cannot be written to before the fourth of ten
  moves
- **THEN** that move is not made, the run aborts, and the three moves
  already made are in the log

### Requirement: Dry-run logs are optional and best-effort
Preview runs SHALL write run logs by default; `--no-run-log` (or its
config key) disables them, and a failure to write one warns without
failing the run.

#### Scenario: Suppressed dry-run log
- **WHEN** a preview run executes with `--no-run-log`
- **THEN** no run-log file is created and the run completes normally

### Requirement: Apply-run logs fall back to XDG state outside a collection
An apply run on files outside any collection root SHALL write its run
log under the XDG state directory instead, so that an applied rename
leaves a readable record of what it moved wherever the files were.

#### Scenario: Apply in a downloads directory
- **WHEN** an apply run renames files in a directory with no
  `.borax.toml` above it
- **THEN** the run log is written under the XDG state directory,
  carrying the original path, the new path and the content hash of
  every rename the run applied

### Requirement: An interactive rename run is logged as an applying run
A rename run that is interactive SHALL be treated as an applying run by every run-log requirement, because it can move files: its log is mandatory, is named with the `apply` suffix, falls back to the XDG state directory outside a collection, and its `run-started` event reports `applying: true`.

The log SHALL be created, and its first event written, before the first
question is put, so that a run unable to record what it moves is refused
before the operator has answered anything.

#### Scenario: Interactive run with an unwritable log
- **WHEN** an interactive rename run cannot create its run log
- **THEN** the run aborts with a clear error before any question is put,
  and every file keeps its original name

#### Scenario: A session that moves nothing
- **WHEN** an interactive run ends with every proposal declined
- **THEN** its run log exists, is named with the `apply` suffix, and
  records each file as skipped with reason `declined`

