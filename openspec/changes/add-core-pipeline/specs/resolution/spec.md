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

#### Scenario: Metadata conflict
- **WHEN** a file's embedded title and the Crossref record's title
  disagree materially
- **THEN** the file is skipped with reason "metadata conflict" and no
  rename is planned for it

### Requirement: The run summary reports the skip queue
Every run SHALL end with a summary listing each skipped file with its
reason, in both human and JSON output modes.

#### Scenario: Summary after a mixed batch
- **WHEN** a batch resolves 8 files and skips 2
- **THEN** the summary lists the 2 skipped files with their reasons, and
  the JSON stream contains one skip event per skipped file
