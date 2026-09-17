# resolution Specification

## Purpose
TBD - created by archiving change add-core-pipeline. Update Purpose after archive.
## Requirements
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

A record an operator accepted over a conflict SHALL NOT be judged
again. It is written to the content index under the file's content
hash, and a later run resolving that file from the index SHALL use it
as it uses any record the index holds, without re-reading the file or
re-checking its titles: the decision was made once, by the only party
entitled to make it.

Only a person SHALL be able to accept a conflicting record. In an
interactive rename run, a file with a conflict SHALL be shown to the
operator with the extracted title, the record's title and their
similarity, and the operator MAY accept the record by choosing to rename
the file to the target it names. No setting, flag or default SHALL
accept one, and a batch run SHALL skip every conflict as this
requirement describes.

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

#### Scenario: Operator accepts a conflict
- **WHEN** an interactive run shows a file whose manuscript title differs
  from its published record's, and the operator chooses to rename it
  anyway
- **THEN** the file is renamed from that record, and its `resolved` event
  carries the conflict it overrode with the reported similarity

#### Scenario: An accepted conflict is not re-judged
- **WHEN** a file whose record an operator accepted over a conflict is
  reached again by a batch run
- **THEN** it resolves from the content index and is not skipped as a
  conflict

#### Scenario: Batch still skips
- **WHEN** the same file is reached by a batch run with `--apply`
- **THEN** it is skipped with reason "metadata conflict" and not renamed

### Requirement: The run summary reports the skip queue
Every run SHALL end with a summary listing each skipped file with its
reason, in both human and JSON output modes.

#### Scenario: Summary after a mixed batch
- **WHEN** a batch resolves 8 files and skips 2
- **THEN** the summary lists the 2 skipped files with their reasons, and
  the JSON stream contains one skip event per skipped file

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

### Requirement: An operator can supply an identifier
In an interactive rename run, the operator SHALL be able to supply an identifier for a file, and resolution SHALL treat a supplied identifier as it treats an extracted one: parsed and normalised by the same rules, dispatched to the same services in the same priority order, and checked against the titles the file claims.

A supplied identifier SHALL be accepted as a DOI in any form extraction
accepts, as an arXiv identifier, or, with a `pmid:` or `isbn:` prefix,
as a PMID or ISBN. Input that is none of these SHALL be refused with the
forms accepted and SHALL NOT be sent to any service.

A record resolved from a supplied identifier SHALL be described to the
operator and used only on the operator's further answer. A disagreement
between it and the file's claimed titles SHALL be shown and SHALL NOT
by itself prevent that answer.

A `resolved` event for a record found from a supplied identifier SHALL
report `supplied` as the pass that found the identifier.

#### Scenario: A preprint without a DOI
- **WHEN** an interactive run finds no identifier in a file and the
  operator supplies `arXiv:2401.12345`
- **THEN** the arXiv record is resolved, described, and proposed as the
  file's name, and nothing moves until the operator answers

#### Scenario: A pasted DOI link
- **WHEN** the operator supplies `https://doi.org/10.1021/JACS.4C01234`
- **THEN** the identifier resolved is the DOI `10.1021/jacs.4c01234`

#### Scenario: Input that names nothing
- **WHEN** the operator supplies `see email from Anna`
- **THEN** it is refused with the accepted forms, no service is queried,
  and the operator is asked again

#### Scenario: An unknown supplied identifier
- **WHEN** the operator supplies a well-formed DOI that no service holds
- **THEN** the services' answers are shown and the file's question is put
  again

### Requirement: An operator's answer about a file is remembered
When an interactive run renames a file from a record found from a supplied identifier, or from a record accepted over a conflict, it SHALL write that record to the content index under the file's content hash, so that a later run resolves the file from it as from any record the index holds.

No record SHALL be written for a file the operator did not rename: a
record resolved from a supplied identifier and then skipped, abandoned
for another identifier, or left by quitting SHALL NOT be stored. What
the index already held for that file SHALL be left as it is — the
rejected candidate is not kept, and nothing the file was identified as
before is removed.

Remembering is best-effort, as every write to the response cache is: an
entry that could not be written means the file is asked about again,
which is how losing an answer fails safely. A run SHALL NOT report a
write it could not make as a failure of the rename it followed.

#### Scenario: Asked once
- **WHEN** a file is renamed in an interactive run from a supplied
  identifier, and a batch run later reaches it under its new name
- **THEN** the batch run resolves it from the content index without
  extraction or any source being queried, and reports it already named

#### Scenario: A wrong identifier abandoned
- **WHEN** the operator supplies an identifier, sees a record for a
  different paper, and skips the file
- **THEN** the content index holds no record for the file, and the next
  interactive run asks about it again

#### Scenario: The answer could not be kept
- **WHEN** a rename from a supplied identifier is made and the content
  index cannot be written
- **THEN** the rename stands and is reported as any rename is, and the
  next run over that file asks about it again

