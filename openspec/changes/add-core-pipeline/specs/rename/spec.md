# rename — delta for add-core-pipeline

## ADDED Requirements

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

### Requirement: Applied renames are journaled
Every applied rename SHALL append a journal entry (original path, new
path, file content hash, timestamp, run identifier) to an append-only
journal in the XDG state directory before the run reports success.

#### Scenario: Journal written on apply
- **WHEN** a run applies three renames
- **THEN** the journal gains three entries sharing one run identifier

### Requirement: Undo reverts the last applied run safely
`borax undo` SHALL revert the renames of the most recent applied run in
reverse order. Before reverting each entry it SHALL verify the file still
exists at the journaled new path with the journaled content hash; entries
failing verification are reported and left untouched rather than guessed
at.

#### Scenario: Clean undo
- **WHEN** `borax undo` runs after an applied run and no files were
  touched since
- **THEN** every file is restored to its original path

#### Scenario: File moved after the run
- **WHEN** one renamed file was moved away before `borax undo`
- **THEN** that entry is reported as unrevertible and all other entries
  are still reverted
