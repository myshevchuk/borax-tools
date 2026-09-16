## MODIFIED Requirements

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
