## MODIFIED Requirements

### Requirement: A template may file a document into a subdirectory
A rendered name containing `/` SHALL be treated as a path relative to
the directory the file belongs in, and any part of it that does not
exist SHALL be created when the rename is applied. Sanitization
prevents such a target from leaving that directory.

The directory a file belongs in is the collection root, where the run
has one. A rendered subdirectory therefore says where in the collection
a file belongs rather than adding a level to wherever it sits now: a
file already filed there is at its target and is already named, one
filed under a subdirectory the template no longer renders moves across
into the new one rather than deeper into the old, and one further down
the tree is filed where it belongs.

A run with no collection root SHALL join a rendered subdirectory to the
file's own directory, having no other base to use.

A rendered name containing no `/` SHALL leave the file where it is,
whatever the run's base: a rendered subdirectory says where in the
collection a file belongs, and a plain name says what to call it where
it already sits.

Collisions SHALL be detected across every file a run files into one
directory, whichever directories those files came from. A run filing
two files from different directories into one journal directory SHALL
suffix the second exactly as it would if both had started in the same
place, and SHALL do so when previewing as well as when applying.

Collisions SHALL be detected where the file is going, not where it came
from: a name already taken in the target subdirectory blocks or suffixes
exactly as one in the file's own directory does, and two files heading
for the same name in different subdirectories do not collide.

#### Scenario: Filing by journal
- **WHEN** a template renders `nature/smith2024` for a file in `~/lib`
- **THEN** the file is moved to `~/lib/nature/smith2024.pdf`, creating
  `~/lib/nature` if it is not there

#### Scenario: The nested name is taken
- **WHEN** a different file already sits at the nested target
- **THEN** it is suffixed or skipped by the collision policy, exactly as
  it would be in the file's own directory

#### Scenario: Re-running a filed collection changes nothing
- **WHEN** a run files `zeng2026.pdf` into `Nature/` under the
  collection root and the same run is repeated over the same collection
- **THEN** the file is reported already named and stays at
  `Nature/zeng2026.pdf`, rather than being proposed
  `Nature/Nature/zeng2026.pdf`

#### Scenario: Filing outside a collection
- **WHEN** a run over a directory with no `.borax.toml` above it renders
  `sub/smith2024` for a file in that directory
- **THEN** the file is filed into `sub/` beneath it, as it is today

#### Scenario: The journal a file is filed under changes
- **WHEN** a file sits at `Nature/zeng2026.pdf` and its template now
  renders `Science/zeng2026`
- **THEN** the file is proposed `Science/zeng2026.pdf` beside the
  `Nature` directory, not inside it

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

### Requirement: A collision suffix never lands on the file's own name
A collision suffix candidate equal to the name the file already carries SHALL be reported as already named rather than planned as a move. A file's own name is exempt from the collision check while its own decision is made, so the suffix ladder can reach it; arriving there means the file already sits where the plan would put it.

#### Scenario: The second of two works, re-run
- **WHEN** two works render `smith2024.pdf`, the second was filed as
  `smith2024a.pdf`, and the directory is run again
- **THEN** `smith2024a.pdf` is reported already named, and no move onto
  its own name is planned or attempted

#### Scenario: A plain name does not move a filed file
- **WHEN** a template rendering no `/` is applied to a file sitting in a
  subdirectory of the collection root
- **THEN** the file is renamed where it is, not hoisted to the root

#### Scenario: Two directories filing into one
- **WHEN** two files in different directories of one collection both
  render `Nature/zeng2026`
- **THEN** the second is suffixed, in a preview and in an applying run
  alike
