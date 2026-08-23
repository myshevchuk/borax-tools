# rename — delta for add-ledger-and-run-logs

## RENAMED Requirements

- FROM: `### Requirement: Applied renames are journaled`
- TO: `### Requirement: Applied renames are recorded in the run log`

## MODIFIED Requirements

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
