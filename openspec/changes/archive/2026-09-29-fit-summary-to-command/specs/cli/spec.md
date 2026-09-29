## ADDED Requirements

### Requirement: A run's human summary fits its command
The human rendering of `run-finished` SHALL be a summary shaped for the command the run's `run-started` names, and SHALL carry no counter that command cannot produce.

The shapes are these, with each optional clause written only when its
count is nonzero:

- `rename`, batch and interactive: `N resolved, N renamed, N skipped`,
  then the optional `already named` clause (with how many were not
  shown, for an interactive run that passed files over), `unmatched`,
  `not reached` and `findings` clauses.
- `resolve` and `bib`: `N resolved, N skipped`, then the optional
  `unmatched`, `not reached` and `findings` clauses. Neither command
  renames a file or finds one already named, so neither says so.
- `status` with or without `--identify`, `validate`, `reconcile`,
  `adopt`, `config` and `cache`: no summary line. Each ends on its own
  report or totals line, so a reader still sees where the run ended.

The human rendering SHALL name every nonzero total that makes a run end
in partial success — skipped files, files not reached, and findings —
so a person is never shown a clean-looking run that exited with the
partial-success code. A command's summary shape names each such total
when it is nonzero, with one exception: `validate` reports its findings
on its own totals line and on a line per finding, and its summary does
not repeat them. A command with no summary line whose run produces
such a total that its own lines do not state SHALL write a summary line
naming it, so that the rule holds for a total a later change lets that
command produce.

This is a change to the human rendering alone. The `run-finished` event
SHALL carry the same counters in `--json` output and in the run log
whatever the command, and the event schema version does not change.
Human output remains a rendering of the same event stream: which
summary is written depends only on the command the stream's own
`run-started` names.

#### Scenario: A library report ends on the report
- **WHEN** `borax status` runs over a library holding seven artifacts
  and no records
- **THEN** the last line of human output is the report naming seven
  artifacts and seven orphans, and no line counts anything resolved,
  renamed or skipped

#### Scenario: Validation findings are on the validation line
- **WHEN** `borax validate` runs over a library with two findings
- **THEN** each finding is reported on a line of its own, the last line
  of human output is the totals line stating two findings, no summary
  line follows it, and the exit code is the partial-success code

#### Scenario: Resolve reports resolution totals
- **WHEN** `borax resolve` resolves one file and skips none
- **THEN** the last line of human output is `1 resolved, 0 skipped`

#### Scenario: Resolve names its skips
- **WHEN** `borax resolve` resolves one file and skips one
- **THEN** the last line of human output is `1 resolved, 1 skipped` and
  the exit code is the partial-success code

#### Scenario: A rename summary is unchanged
- **WHEN** a batch `borax rename` resolves one file that already carries
  its name
- **THEN** the last line of human output is
  `1 resolved, 0 renamed, 0 skipped, 1 already named`

#### Scenario: JSON keeps every counter
- **WHEN** `borax status --json` runs
- **THEN** its last line is a `run-finished` event carrying `resolved`,
  `renamed`, `skipped`, `named`, `unmatched`, `unreached` and
  `findings`, exactly as before this change, under the same schema
  version

#### Scenario: The run log keeps every counter
- **WHEN** `borax status` runs in human mode inside a library whose run
  log is on
- **THEN** the run log closes with a `run-finished` event carrying all
  seven counters, though the terminal showed no summary line
