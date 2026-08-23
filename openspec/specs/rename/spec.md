# rename Specification

## Purpose
TBD - created by archiving change add-core-pipeline. Update Purpose after archive.
## Requirements
### Requirement: Preview is the default; apply is explicit
A rename run SHALL, by default, print the planned old→new mapping and
mutate nothing on disk. Renames SHALL only be executed when `--apply` is
given.

#### Scenario: Default run is a preview
- **WHEN** `borax rename` runs over a directory without `--apply`
- **THEN** the old→new mapping is printed and every file keeps its
  original name

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
apply-run log — the mandatory, pre-flushed record defined by the
`run-logs` capability. The events for the whole plan are flushed to the
log before the first rename executes; there is no separate journal file.

#### Scenario: Rename events written on apply
- **WHEN** a run applies three renames
- **THEN** the apply-run log contains three rename events sharing one
  run identifier, flushed before any file was renamed

### Requirement: Undo reverts the last applied run safely
`borax undo` SHALL revert the renames of the most recent apply-run log —
searching the current collection's `.borax/runs/` first, then the XDG
state fallback — replaying its rename events in reverse order. Before
reverting each entry it SHALL verify the file still exists at the
recorded new path with the recorded content hash; entries failing
verification are reported and left untouched rather than guessed at.
Undo SHALL refuse, with a clear error, a run log whose event-schema
version it does not understand.

#### Scenario: Clean undo
- **WHEN** `borax undo` runs after an applied run and no files were
  touched since
- **THEN** every file is restored to its original path

#### Scenario: File moved after the run
- **WHEN** one renamed file was moved away before `borax undo`
- **THEN** that entry is reported as unrevertible and all other entries
  are still reverted

#### Scenario: Unknown schema version
- **WHEN** the latest apply-run log carries an event-schema version
  newer than the running binary understands
- **THEN** undo aborts with an error naming the version mismatch and
  reverts nothing

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

