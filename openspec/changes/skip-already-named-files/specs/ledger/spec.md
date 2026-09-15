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
