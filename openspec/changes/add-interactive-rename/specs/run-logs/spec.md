## MODIFIED Requirements

<!-- drops: the whole-plan pre-flush, replaced by the per-move guarantee
     the rename capability now states; a run that decides one file at a
     time has no whole plan to flush -->

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

## ADDED Requirements

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
