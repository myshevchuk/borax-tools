# ledger Specification

## Purpose
TBD - created by archiving change add-ledger-and-run-logs. Update Purpose after archive.
## Requirements
### Requirement: The ledger is an append-only JSONL file at the collection root
The ledger SHALL be a single append-only JSON Lines file at
`.borax/ledger.jsonl` under the collection root, one object per admitted
file, each carrying at minimum: content hash, normalized identifiers,
final path relative to the collection root, entry type, run id,
timestamp, and tool version. Entries SHALL be appended only by applied
(non-preview) admissions.

#### Scenario: Applied rename appends an entry
- **WHEN** an apply run renames a resolved file into the collection
- **THEN** `.borax/ledger.jsonl` gains one entry recording the file's
  hash, identifiers, and collection-relative path

#### Scenario: Preview appends nothing
- **WHEN** the same run executes without `--apply`
- **THEN** the ledger file is not modified

### Requirement: Duplicate detection operates at two levels with distinct reasons
The pipeline SHALL check each incoming file against the ledger twice and
report the two outcomes distinctly: a content duplicate (the file's hash
already has a ledger entry) is reported as "same bytes already archived"
naming the existing path; a work duplicate (hash unknown, but a resolved
identifier already has a ledger entry) is reported as "same work already
archived (different file)" naming the existing path. The content check
SHALL run after hashing and before any resolution, so byte-identical
re-downloads cost no network access.

A ledger entry whose recorded path is the incoming file's own path SHALL
NOT make that file a duplicate at either level. Such an entry records the
file itself, not another copy of it, and the file SHALL continue through
resolution and planning as a file matching no entry does.

The lookup SHALL pass over that entry and go on, rather than the
result being discarded once found: the ledger answers with one entry
per hash and one per identifier, so a file whose own entry is that
answer would otherwise hide a second copy recorded elsewhere in the
collection.

Paths SHALL be compared as the collection builds them — the entry's
collection-relative path under the collection root against the
incoming path made absolute — normalised lexically and matched the way
the platform matches file names. Symlinks SHALL NOT be resolved.

#### Scenario: Re-downloaded identical file
- **WHEN** an incoming file's hash matches a ledger entry
- **THEN** it is reported as a content duplicate of the recorded path
  and no source is queried for it

#### Scenario: Second PDF of an archived paper
- **WHEN** an incoming file's hash is unknown but its resolved DOI
  matches a ledger entry
- **THEN** it is reported as a work duplicate naming the recorded path

#### Scenario: Re-running over an admitted file
- **WHEN** a file an applied run renamed and admitted is reached again
  at the path the ledger records for it
- **THEN** it is not reported as a duplicate, and with its record in the
  content index it is reported already named without any source being
  queried

#### Scenario: An admitted file annotated afterwards
- **WHEN** a file at its admitted path has changed bytes but resolves to
  the identifier its entry records
- **THEN** it is not reported as a work duplicate of itself

#### Scenario: A file's own entry does not hide a copy
- **WHEN** a file at its admitted path is reached again and a
  byte-identical copy of it sits elsewhere in the collection under its
  own ledger entry
- **THEN** the incoming file is reported a content duplicate of that
  other copy, not passed as new

### Requirement: Duplicates are skipped, never destroyed
Detected duplicates SHALL divert to the skip queue with their duplicate
reason and the existing path; the incoming file is left untouched and
nothing in the collection is deleted, overwritten, or replaced.

#### Scenario: Duplicate in a batch
- **WHEN** a batch contains one duplicate among new files
- **THEN** the duplicate's source file still exists unmodified after the
  run and every other file is processed normally

### Requirement: Stale entries never block re-admission
A duplicate report SHALL first verify that the entry's recorded path
still exists in the collection; if it does not (the file was deleted or
moved), the entry is stale — the incoming file is processed normally
and a warning suggests `borax ledger rebuild`. Disk is the source of
truth; the ledger alone never vetoes an admission.

#### Scenario: Duplicate of a vanished admission
- **WHEN** an incoming file matches a ledger entry whose recorded path
  no longer exists
- **THEN** the file is processed normally and the run warns that the
  ledger holds stale entries

### Requirement: The ledger is rebuildable and rebuilds deterministically
`borax ledger rebuild` SHALL regenerate the ledger by scanning the
collection's files and their sidecars, writing entries sorted by
collection-relative path. Rebuilding an unchanged collection twice SHALL
produce byte-identical files, and a rebuild SHALL compact away entries
whose files no longer exist.

#### Scenario: Rebuild after manual deletions
- **WHEN** files recorded in the ledger were deleted from the collection
  and `borax ledger rebuild` runs
- **THEN** the regenerated ledger contains no entries for the deleted
  files

#### Scenario: Rebuild is idempotent
- **WHEN** `borax ledger rebuild` runs twice on an unchanged collection
- **THEN** the second output is byte-identical to the first

### Requirement: The ledger is optional and degrades loudly, never blocking
The pipeline SHALL treat the ledger as derived accounting: when it is
disabled (`--no-ledger` or config), absent, unparsable, or there is no
collection root, the run proceeds with duplicate detection off and — in
the absent/unparsable cases — a single warning naming the cause. A torn
trailing line (interrupted append) SHALL be ignored with a warning
rather than failing the parse.

#### Scenario: Corrupt ledger
- **WHEN** `.borax/ledger.jsonl` contains an unparsable line mid-file
- **THEN** the run warns that duplicate detection is off and completes
  normally, and `borax ledger rebuild` restores a valid ledger

#### Scenario: No collection root
- **WHEN** files are processed in a directory tree with no
  `.borax.toml` above it
- **THEN** no ledger is read or written and no duplicate warning is
  possible for that run

