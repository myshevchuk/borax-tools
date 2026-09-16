# rename Specification

## Purpose
TBD - created by archiving change add-core-pipeline. Update Purpose after archive.
## Requirements
### Requirement: Preview is the default; apply is explicit
A rename run SHALL move no file without an explicit decision to move it.
There are exactly two such decisions: `--apply`, given to a batch run,
which carries out the plan that run would otherwise preview; and a yes,
given in an interactive run to a question naming the file and the target
it would move to, which carries out that one move. No default, no
configuration value and no answer that was never put to a person is a
decision.

A batch rename run SHALL, by default, print the planned old→new mapping
and mutate nothing on disk. Renames SHALL only be executed in a batch
run when `--apply` is given.

An interactive run SHALL show each proposed move before it happens, as
the question that decides it. A batch run SHALL show one when it is
previewing, which is the run an `--apply` run repeats; `--apply` itself
carries out the plan without showing it again.

#### Scenario: Default run is a preview
- **WHEN** `borax rename --batch` runs over a directory without
  `--apply`
- **THEN** the old→new mapping is printed and every file keeps its
  original name

#### Scenario: An unanswered question moves nothing
- **WHEN** an interactive run is proposing a move and the operator has
  not yet answered
- **THEN** the file keeps its original name

### Requirement: Collisions never overwrite
A planned rename whose target path already exists SHALL never overwrite
the existing file. The planner SHALL detect collisions among the batch's
own targets and against existing files, including on case-insensitive
filesystems. Per configuration, a collision is resolved by suffixing
(deterministic `a`, `b`, `c`… suffixes) or by skipping with a reported
reason. A target that already contains a byte-identical file SHALL be
reported as already-named rather than treated as a collision.

#### Scenario: Two records render the same filename
- **WHEN** two different files in one batch render identical target names
- **THEN** with suffixing configured, the second receives a deterministic
  suffix, and with skipping configured, the second is skipped with reason
  "target collision"

#### Scenario: Case-insensitive collision
- **WHEN** a target differs from an existing file's name only by letter
  case
- **THEN** the planner treats it as a collision on all platforms

### Requirement: A template may file a document into a subdirectory
A rendered name containing `/` SHALL be treated as a path relative to
the file's own directory, and any part of it that does not exist SHALL
be created when the rename is applied. Sanitization prevents such a
target from leaving the file's directory.

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

### Requirement: Applied renames are recorded in the run log
Every applied rename SHALL be recorded as a rename event (original path,
new path, file content hash, timestamp, run identifier) in the run's
apply-run log — the mandatory record defined by the `run-logs`
capability. There is no separate journal file.

Each such event SHALL be written and flushed to the log before the move
it records is made, so that a run interrupted at any point has already
recorded every move it may have made. A move whose event cannot be
written SHALL NOT be made.

A move that then fails SHALL be reported as it is today, and its failure
SHALL be written to the log after the rename event it followed, so a
reader of the log sees both the intent and its outcome.

#### Scenario: Rename events written on apply
- **WHEN** a run applies three renames
- **THEN** the apply-run log contains three rename events sharing one
  run identifier, each written before the file it names was moved

#### Scenario: A move recorded and then refused
- **WHEN** the filesystem refuses a move whose rename event was already
  written
- **THEN** the log carries that rename event followed by the skip
  recording the failure

### Requirement: A run reports one file at a time
A run SHALL report each file completely before it begins the next: the
file's resolution, the rename planned or applied for it, and any sidecar
written beside it appear together and in the order the files were given.
A run SHALL NOT report every file's resolution first and every file's
rename afterwards, which leaves the two lists unpairable — the second
omits the files that were skipped, and a reader cannot recover which
verdict belongs to which file.

Bibliography output written to a single shared destination is exempt,
being work about the batch rather than about a file.

Ordering is the only thing this constrains. Which name a file receives
SHALL NOT depend on it: collision suffixes stay deterministic, and a
preview stays identical to what the same run with `--apply` would do.

#### Scenario: A file's verdict and its fate are adjacent
- **WHEN** `borax rename` runs over a directory where some files
  resolve and others are skipped
- **THEN** each resolved file's rename line immediately follows its own
  resolution line, and no file's lines are separated by another file's

#### Scenario: Ordering does not move a suffix
- **WHEN** two files in one directory render the same target name
- **THEN** the first file given receives the unsuffixed name and the
  second the suffix, exactly as when the batch was planned as a whole

### Requirement: A rename run on a terminal is interactive
A rename run SHALL be interactive when its standard input is a terminal, its output is human-readable, `--apply` was not given, and the `rename.batch` setting is off, which is its default. Every other rename run SHALL be a batch run, behaving as a rename run behaved before interactive runs existed.

`--apply` SHALL select a batch run by itself, so that `borax rename
--apply <path>` carries out a plan without asking whatever the setting
says. Giving `--apply` together with `--no-batch` SHALL be a usage error
naming both flags, refused before anything is read or resolved, because
the pair asks both for a plan carried out unasked and for every move to
be asked about.

The mode SHALL be decided before the run's first event and SHALL NOT
change during the run.

#### Scenario: Terminal default
- **WHEN** `borax rename papers/` runs from a terminal with no setting
  or flag selecting batch
- **THEN** the run is interactive and `run-started` reports
  `interactive: true`

#### Scenario: Apply keeps its meaning
- **WHEN** `borax rename --apply papers/` runs from a terminal
- **THEN** the run is a batch run that moves every file its plan names,
  asking nothing

#### Scenario: Configured batch default
- **WHEN** configuration sets `rename.batch = true` and `borax rename
  papers/` runs from a terminal
- **THEN** the run is a batch preview, and `borax rename --no-batch
  papers/` from the same terminal is interactive

#### Scenario: Piped and JSON runs never ask
- **WHEN** `borax rename --no-batch papers/` runs with stdin redirected,
  or `borax rename --json papers/` runs from a terminal
- **THEN** the run is a batch preview and asks nothing

#### Scenario: Apply against no-batch
- **WHEN** `borax rename --apply --no-batch papers/` runs
- **THEN** the invocation is rejected as a usage error naming both flags,
  no event is emitted, and the exit code is the fatal one

### Requirement: An interactive run asks before each move
An interactive run SHALL put a question to the operator for each file whose plan is a move to a free target, before the file is moved, cited or admitted. The question SHALL name the file and the target it would move to, including any collision suffix, and SHALL offer at least: rename, skip, and quit.

A rename answer SHALL carry out the move as an applying batch run would
carry it out, and report it the same way. A skip answer SHALL leave the
file untouched and report it skipped with reason `declined`.

A proposed target SHALL be claimed only when the operator accepts it. A
declined proposal SHALL leave the run's plan as though that file had
proposed nothing, so no later file is suffixed on account of a move that
did not happen.

A file whose plan is not a move — already named, blocked by a collision
under the skip policy, or unnameable — and a file that did not resolve
SHALL be reported as a batch run reports it, without a question, unless
another requirement gives it one.

Files SHALL be asked about in the order a batch run would report them,
and each file's resolution, question, fate and sidecar SHALL remain
adjacent.

#### Scenario: Accepting a proposal
- **WHEN** the operator answers rename to the question for `a.pdf →
  smith2024.pdf`
- **THEN** the file is moved, a `renamed` event names both paths and the
  content hash, and the collection's ledger admits it as an applied
  rename would

#### Scenario: Declining a proposal
- **WHEN** the operator answers skip to the question for `a.pdf`
- **THEN** `a.pdf` keeps its name and is reported skipped with reason
  `declined`

#### Scenario: A declined name stays free
- **WHEN** `a.pdf` and `b.pdf` in one directory both render
  `smith2024.pdf`, and the operator declines `a.pdf`
- **THEN** the question for `b.pdf` proposes `smith2024.pdf` without a
  suffix

#### Scenario: A file with nothing to decide
- **WHEN** an interactive run reaches a file that already carries the
  name its record implies
- **THEN** it is reported as already named and no question is put

### Requirement: Quitting an interactive run leaves the rest untouched
Answering quit, or interrupting a question, SHALL end an interactive run at that file: the file and every file after it SHALL be left untouched, nothing further SHALL be resolved, and the run SHALL close with `run-finished` as a completed run does.

`run-finished` SHALL count as unreached the file the run ended at and
every file after it, since none of them was given a fate. A run that
leaves any file unreached SHALL exit with the partial-success code.

Work already done SHALL stand: files moved before the quit stay moved
and recorded, and bibliography output for the files that were visited is
written as it would have been had the run finished.

#### Scenario: Quit midway
- **WHEN** the operator quits at the third of ten files, having renamed
  the first two
- **THEN** the first two stay renamed, the other eight keep their names,
  `run-finished` counts eight unreached, and the exit code is the
  partial-success code

#### Scenario: Interrupt at a prompt
- **WHEN** the operator presses Ctrl-C while a question is showing
- **THEN** the run ends as it would on quit, closing with `run-finished`

