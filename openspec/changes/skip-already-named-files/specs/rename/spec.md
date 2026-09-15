## ADDED Requirements

### Requirement: A file already carrying its name is not a skip
A file whose current name is the name its record implies under the configuration in force, or whose target is occupied by a byte-identical file, SHALL be reported as an `already-named` outcome of its own, and SHALL NOT be reported or counted as skipped.

`run-finished` SHALL count already-named files in a total of their own.
Already-named files SHALL NOT make a run exit with the partial-success
code: a run over files that are all already named exits 0.

Whether a file is already named SHALL be decided by rendering its record
through the templates in force for its directory, never by whether a
ledger records it, so a file named under an earlier template is not
already named under a changed one.

#### Scenario: Re-run over a folder in order
- **WHEN** `borax rename --batch` runs over a directory in which every
  file already carries the name its record implies
- **THEN** each file is reported `already-named`, `run-finished` counts
  them as named and none as skipped, and the exit code is 0

#### Scenario: Template changed since the last run
- **WHEN** a file was renamed under one filename template and the
  configuration now holds a different one
- **THEN** the file is not already named, and a rename to the new
  template's name is planned for it

### Requirement: An interactive run passes over already-named files
An interactive rename run SHALL, when the `rename.skip-named` setting is on, render nothing to the operator for a file that is already named: no resolution line, no outcome line and no question. The setting SHALL default to on, and `--no-skip-named` SHALL turn it off for a run.

A passed-over file SHALL still be reported in the event stream and the
run log exactly as when the setting is off, and the run's closing
summary SHALL state how many files were passed over.

With the setting off, an interactive run SHALL render already-named
files as a batch run does. The setting SHALL NOT affect batch runs.

#### Scenario: New papers among renamed ones
- **WHEN** an interactive run reaches a directory holding three files
  that need renaming and two hundred already named
- **THEN** the operator sees the three questions and no line about the
  other two hundred, and the summary reports two hundred already named

#### Scenario: Showing named files
- **WHEN** the same run is started with `--no-skip-named`
- **THEN** each already-named file is shown as already named, and no
  question is put for it

#### Scenario: The run log keeps the full account
- **WHEN** an interactive run passes over an already-named file
- **THEN** its run log carries that file's resolution and its
  `already-named` event
