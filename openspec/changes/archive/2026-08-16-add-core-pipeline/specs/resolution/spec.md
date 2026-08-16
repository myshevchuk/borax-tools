# resolution — delta for add-core-pipeline

## ADDED Requirements

### Requirement: Sources are queried by identifier type and priority
Resolution SHALL dispatch on identifier type: DOIs query Crossref first,
falling back to OpenAlex; arXiv identifiers query the arXiv API first. A
source failure (network error, HTTP error, record not found) SHALL fall
through to the next source in priority order; only when all applicable
sources fail is the file diverted to the skip queue.

#### Scenario: Crossref outage falls back to OpenAlex
- **WHEN** resolving a DOI while Crossref returns HTTP 503 and OpenAlex
  holds the record
- **THEN** resolution succeeds with a record whose provenance names
  OpenAlex

#### Scenario: Identifier unknown everywhere
- **WHEN** a syntactically valid DOI is found in no configured source
- **THEN** the file is diverted to the skip queue with reason
  "identifier not resolvable" and the batch continues

### Requirement: Responses are cached locally
Successful source responses SHALL be cached on disk keyed by normalized
identifier, and resolved records SHALL additionally be indexed by the
source file's content hash. A cache hit SHALL be used without any network
request. A bypass flag (`--no-cache`) SHALL force live queries, and a
cache subcommand SHALL be able to clear the cache.

#### Scenario: Re-run over the same directory is offline
- **WHEN** a directory is processed a second time with an intact cache
- **THEN** no network requests are made and results are identical to the
  first run

#### Scenario: Renamed file, same content
- **WHEN** a previously resolved file is encountered again under a
  different name with identical content
- **THEN** its record is served from the content-hash index without
  re-extraction or network access

### Requirement: Network use is polite and bounded
Every request SHALL carry a User-Agent identifying borax and its version
and, when configured, a contact mailto (Crossref/OpenAlex polite pools).
Requests SHALL be rate-limited per source, and concurrent resolution
across files SHALL be bounded by a configurable limit.

Concurrency SHALL NOT change what a run reports: results are ordered by
their input position, so the event stream of a concurrent run is
identical to a sequential one's over the same inputs. Rate limiting is
per service and independent of the number of threads in flight.

#### Scenario: Concurrency does not reorder the stream
- **WHEN** the same batch is resolved with a concurrency of one and with
  a concurrency of eight
- **THEN** both runs emit the same events in the same order

#### Scenario: Polite-pool identification
- **WHEN** a contact address is set in configuration and a Crossref
  request is issued
- **THEN** the request carries the configured mailto and the borax
  User-Agent

### Requirement: Ambiguity is skipped, never guessed
Resolution SHALL divert files with conflicting or low-confidence results
(e.g. embedded metadata and the resolved record disagreeing on title
beyond a normalization tolerance) untouched to the skip queue with a
stated reason. Batch mode SHALL NOT auto-accept a low-confidence match.

The tolerance SHALL be a similarity threshold, not equality. Titles SHALL
be compared as content words after folding both sides toward the lossiest
encoding either could carry — Latin letters transliterated to ASCII,
every other non-alphanumeric character (punctuation, dashes, and letters
with no transliteration) treated as a separator, function words dropped —
and SHALL be taken to name the same work when either is a prefix of the
other, when they differ only in where words were split, or when their
similarity reaches the threshold. The skip event SHALL report the
similarity, so a near miss is distinguishable from two unrelated works.

A document may claim several titles (an XMP `dc:title` and an Info
dictionary title, which need not agree). Every claim SHALL be considered,
and agreement by any one of them SHALL clear the file. A claim that does
not plausibly name a work — a placeholder, a filename, a bare identifier,
or a short value sharing nothing with the record — SHALL NOT count as
evidence of disagreement, since a producer's leftover contradicts every
record and would otherwise make a resolvable file permanently unskippable.

#### Scenario: Metadata conflict
- **WHEN** a file's embedded title and the Crossref record's title
  disagree materially
- **THEN** the file is skipped with reason "metadata conflict", the
  reported similarity is below the threshold, and no rename is planned
  for it

#### Scenario: A character the producer could not encode
- **WHEN** a file's embedded title is the record's title with a Greek
  letter and its hyphens dropped, as a typesetter that cannot encode them
  writes it
- **THEN** the titles agree and the file resolves

#### Scenario: Placeholder alongside a real title
- **WHEN** a file's XMP `dc:title` is a producer's placeholder and its
  Info dictionary carries the real title
- **THEN** the placeholder is not treated as evidence, the real title
  agrees, and the file resolves

### Requirement: The run summary reports the skip queue
Every run SHALL end with a summary listing each skipped file with its
reason, in both human and JSON output modes.

#### Scenario: Summary after a mixed batch
- **WHEN** a batch resolves 8 files and skips 2
- **THEN** the summary lists the 2 skipped files with their reasons, and
  the JSON stream contains one skip event per skipped file
