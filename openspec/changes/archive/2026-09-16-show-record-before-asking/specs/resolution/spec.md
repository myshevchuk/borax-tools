## ADDED Requirements

### Requirement: A resolution reports the evidence it was checked against
A `resolved` event SHALL carry every title the file claims for itself, each with where it was read — the XMP packet or the document information dictionary — in the order they were read, including claims the conflict check did not count as evidence. A file that was not opened, because the content index answered, SHALL carry no claims.

A `resolved` event SHALL carry the identifier the run looked up,
alongside the identifiers the record itself holds. The two are not
always the same — a record resolved from an arXiv identifier may carry
a DOI, which is what the event reports as the record's — and only the
former is evidence about the file.

A `resolved` event SHALL name as its source the services that supplied
the record. When the content index answered, those are the sources the
record's per-field provenance names other than extraction itself, in a
fixed order that does not depend on the identifier type: Crossref,
OpenAlex, arXiv, DataCite, PubMed, then a sidecar. A record whose
provenance names no such source SHALL report the content index itself.
Whether the content index answered SHALL continue to be reported
separately.

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

#### Scenario: The looked-up identifier is carried
- **WHEN** a file resolves from an arXiv identifier to a record that
  also carries a DOI
- **THEN** its `resolved` event reports the arXiv identifier as the one
  found, and the record's DOI as the record's

#### Scenario: A record built from two services
- **WHEN** a record whose provenance names OpenAlex for some fields and
  Crossref for others is served by the content index
- **THEN** its `resolved` event names Crossref before OpenAlex
