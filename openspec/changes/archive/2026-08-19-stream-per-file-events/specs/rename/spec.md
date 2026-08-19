# rename — delta for stream-per-file-events

## ADDED Requirements

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
