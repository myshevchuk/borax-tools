## ADDED Requirements

### Requirement: A resolution reports the evidence it was checked against
A `resolved` event SHALL carry every title the file claims for itself, each with where it was read — the XMP packet or the document information dictionary — in the order they were read, including claims the conflict check did not count as evidence. A file that was not opened, because the content index answered, SHALL carry no claims.

A `resolved` event SHALL name as its source the services that supplied
the record. When the content index answered, those are the services the
record's per-field provenance names, excluding extraction; only a record
whose provenance names no service SHALL report the content index itself
as its source. Whether the content index answered SHALL continue to be
reported separately.

#### Scenario: Claims reach the stream
- **WHEN** a file whose XMP carries a title and whose document
  information carries a producer's placeholder is resolved
- **THEN** its `resolved` event carries both titles, marked XMP and
  document information respectively

#### Scenario: A content-index answer names its service
- **WHEN** a file is resolved from the content index and its record's
  provenance attributes its fields to Crossref
- **THEN** its `resolved` event names Crossref as the source, reports
  that the content index answered, and carries no claims
