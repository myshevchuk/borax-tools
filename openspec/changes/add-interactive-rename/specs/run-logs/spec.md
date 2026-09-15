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
