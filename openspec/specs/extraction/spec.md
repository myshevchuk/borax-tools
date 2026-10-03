# extraction Specification

## Purpose
TBD - created by archiving change add-core-pipeline. Update Purpose after archive.
## Requirements
### Requirement: Tiered extraction stops at the first hit
Identifier extraction SHALL run passes in fixed order — (1) embedded
XMP/document-info metadata, (2) identifier patterns over the text layer —
and SHALL stop at the first pass that yields a valid identifier. Later
passes MUST NOT run once an identifier is found.

#### Scenario: Embedded DOI short-circuits
- **WHEN** a PDF carries a valid DOI in its XMP or document-info metadata
- **THEN** extraction returns that DOI without extracting the text layer

#### Scenario: Fallback to text layer
- **WHEN** a PDF has no identifier in embedded metadata but its text layer
  contains a DOI
- **THEN** extraction returns the DOI found by the text-layer pass

### Requirement: Text-layer pass scans a bounded page range
The text-layer pass SHALL extract text only from the first N pages
(default 3, configurable) and SHALL recognize DOI and arXiv identifier
patterns, including arXiv's pre-2007 and post-2007 ID formats.

#### Scenario: Identifier beyond the scanned range
- **WHEN** the only DOI in a document appears after the configured page
  range
- **THEN** extraction reports "no identifier found" rather than scanning
  the whole document

#### Scenario: arXiv ID recognized
- **WHEN** the first page contains "arXiv:2401.12345v2"
- **THEN** extraction returns the arXiv identifier "2401.12345" with
  version "v2" recorded

### Requirement: Extracted identifiers are validated and normalized
Every candidate identifier SHALL be syntax-validated and normalized before
being returned: DOIs are case-normalized to lowercase with surrounding
punctuation and URL prefixes (e.g. "https://doi.org/") stripped; invalid
candidates are discarded and the pass continues.

#### Scenario: DOI embedded in a URL with trailing punctuation
- **WHEN** the text layer contains "https://doi.org/10.1021/JACS.4C01234."
- **THEN** extraction returns the DOI "10.1021/jacs.4c01234"

### Requirement: Extraction failures are typed and non-fatal
Extraction SHALL distinguish at minimum these failure modes: file
unreadable, PDF encrypted, no text layer, no identifier found. A failure
on one file MUST NOT abort processing of other files in the batch, and the
failure type MUST be reported in the run summary.

#### Scenario: Encrypted PDF in a batch
- **WHEN** a batch contains an encrypted PDF among readable ones
- **THEN** the encrypted file is reported with an "encrypted" failure and
  every other file is still processed

### Requirement: Extraction is offline
Extraction SHALL NOT perform network requests.

#### Scenario: No network during extraction
- **WHEN** extraction runs with networking unavailable
- **THEN** all extraction passes complete normally

### Requirement: Extraction reports one result for each file it is given
Extraction over files a caller selects SHALL yield exactly one result
for each file it is given, in the order given, and SHALL NOT open,
report on or add any file it was not given.

The caller decides which files are extracted. Library context the
caller holds — its root, its stores, its survey — MAY shape how a
result is reported, such as the path it is reported under, and SHALL
NOT add a file to the selection or remove one from it.

The result SHALL be exactly one of the following, and no two of them
SHALL be reported as the same result:

- `found`: a pass found an identifier. The result carries the
  identifier in the form the event stream writes one (`doi:…`,
  `arXiv:…`) and the pass that found it (`embedded-metadata` or
  `text-layer`).
- `no-text-layer`: the metadata held no identifier, and no page the
  text-layer pass read held text, including when it read no page
  because the document has none or the page limit is zero.
- `text-without-identifier`: the metadata held no identifier, and the
  text-layer pass read text that held none.
- `encrypted`: the document cannot be read without a password.
- `unreadable`: the file could not be opened or parsed as a PDF. The
  result carries the reason the reader gave.

A title in the document's metadata SHALL count only through an
identifier the embedded-metadata pass recognises in it, as it already
scans the document-information `Title` among its other fields. A title
holding such an identifier gives `found` with `embedded-metadata`. A
title holding none SHALL NOT change which result a file gets: the file
gets the result its other metadata and its text give it.

The passes run are the ones "Tiered extraction stops at the first hit"
and "Text-layer pass scans a bounded page range" define, which are the
passes resolution runs. Producing a result queries no service, reads
and writes no cache, and writes nothing to a library.

A resolving command's skip reasons are not this result, and this
requirement does not govern them.

#### Scenario: One result per file, and no other file
- **WHEN** extraction is given three files in a directory holding five,
  and the second of the three cannot be opened
- **THEN** there are exactly three results, in the order the files were
  given, the second is `unreadable`, and neither of the other two files
  in the directory is opened

#### Scenario: A title holding no identifier changes nothing
- **WHEN** a PDF's only page is blank and its metadata carries a title
  and no identifier, in the title or anywhere else
- **THEN** its result is `no-text-layer`

#### Scenario: An identifier in the title is found by the metadata pass
- **WHEN** a PDF's only page is blank, its metadata carries no
  identifier outside its title, and its document-information title is
  `10.1234/example`
- **THEN** its result is `found` with `doi:10.1234/example` and
  `embedded-metadata`

#### Scenario: Readable text without an identifier
- **WHEN** a PDF's page holds readable prose and no identifier, and its
  metadata carries a title and no identifier
- **THEN** its result is `text-without-identifier`, not `no-text-layer`

#### Scenario: Encrypted is not unreadable
- **WHEN** one PDF is encrypted under a user password and another is
  truncated so that it cannot be parsed
- **THEN** the first result is `encrypted` and the second is
  `unreadable` with the reader's message

#### Scenario: The pass that found an identifier is named
- **WHEN** one PDF carries a DOI in its XMP packet and another carries
  no identifier in its metadata and prints an arXiv identifier on its
  first page
- **THEN** the first result is `found` with that DOI and
  `embedded-metadata`, and the second is `found` with that arXiv
  identifier and `text-layer`

### Requirement: A resolution keeps extraction's result and the file's titles
A resolution that runs extraction over a file SHALL retain extraction's result as one of the kinds the requirement "Extraction reports one result for each file it is given" defines, and SHALL NOT merge two of those kinds, whatever skip reason the resolution reports.

The result retained by a resolution SHALL be the result that
`borax status --identify` reports for the same file under the same
extraction settings, since both run the same passes.

A resolution SHALL retain the titles the file claims separately from
extraction's result, in exactly one of three states:

- **read**: every title read, each with where it was read, in the
  order read. There MAY be none, which means the file was opened and
  claims no title.
- **failed**: the file could not be opened, retained with the reason.
- **not attempted**: the file was not opened, retained with the reason.

Titles that were read SHALL be retained when extraction then finds no
identifier, or fails while reading the file's pages. A file without an
identifier therefore still carries the titles it claims.

The titles are evidence about the work. Whether a title counts against
a record is the title check's to decide, under the `resolution`
capability. A title still affects extraction's result only through an
identifier the metadata pass finds in it, as before.

#### Scenario: A blank page and readable prose, both titled
- **WHEN** a resolution runs over two PDFs whose document information
  carries the title `A Title` and no identifier, one with a single
  blank page and one with a page of readable prose that holds no
  identifier
- **THEN** the first retains `no-text-layer` and the second
  `text-without-identifier`, both are skipped as before, and both
  retain the titles as read, holding `A Title` read from the document
  information dictionary

#### Scenario: Encrypted is not unreadable
- **WHEN** a resolution runs over a PDF encrypted under a user password
  and over a PDF truncated so that it cannot be parsed
- **THEN** the first retains `encrypted` and the second `unreadable`
  with the reader's message, and both retain their titles as failed,
  each with its reason

#### Scenario: A page that cannot be read after the file opens
- **WHEN** a PDF opens, its document information carries a title, and
  its first page cannot be read
- **THEN** the resolution retains `unreadable` as the result, and the
  title as read

#### Scenario: A file that claims no title
- **WHEN** a PDF with no title in its metadata holds readable prose and
  no identifier
- **THEN** the resolution retains `text-without-identifier`, and the
  titles as read with none, which is distinct from titles that failed
  and from titles not attempted

#### Scenario: Titles not read because the file was not opened
- **WHEN** a file is resolved from the content index
- **THEN** its titles are retained as not attempted because the content
  index answered, not as read with none

### Requirement: A resolution's events report extraction's result and the file's titles
The `extraction` section of every `resolved` event and every resolution `skipped` event SHALL report extraction's `result` as one of the five kinds the requirement "Extraction reports one result for each file it is given" defines, or as not attempted, and the file's `titles` as `read` with their `claims`, `failed` with the reason's `message`, or not attempted, each tagged by `status`.

A `found` result SHALL carry the identifier and the pass that read it,
as `tier`; an `unreadable` result SHALL carry the reader's `message`.
The result SHALL be the one `borax status --identify` reports for the
same file under the same extraction settings.

Titles `read` with no claims SHALL mean the file was opened and claims
no title, and SHALL NOT be reported for a file that could not be opened
or was not opened. The result and the titles are reported independently:
titles read when a file opened SHALL be reported whatever the result.

A resolution skipped because extraction found no identifier SHALL be
reported with a reason whose `kind` names extraction's result —
`no-text-layer`, `text-without-identifier`, `encrypted`, or `unreadable`
with the reader's `message` — and SHALL NOT be reported under a kind
that merges two of them. This is how the failure modes the requirement
"Extraction failures are typed and non-fatal" distinguishes reach the
run's skip queue.

#### Scenario: A blank page and readable prose, both titled
- **WHEN** a resolution runs over two PDFs whose document information
  carries the title `A Title` and no identifier, one with a single blank
  page and one with a page of readable prose that holds no identifier
- **THEN** the first is skipped with reason `no-text-layer` and the
  second with `text-without-identifier`, their `extraction` results are
  the same two kinds, and both report `titles` `read` with `A Title`
  from the document information dictionary

#### Scenario: Encrypted is not unreadable
- **WHEN** a resolution runs over a PDF encrypted under a user password
  and over a PDF truncated so that it cannot be parsed
- **THEN** the first is skipped with reason `encrypted` and the second
  with `unreadable` carrying the reader's message, and both report
  `titles` `failed` with the reason the file could not be opened

#### Scenario: A file that claims no title
- **WHEN** a PDF with no title in its metadata holds readable prose and
  no identifier
- **THEN** its skip reports `text-without-identifier`, and `titles`
  `read` with no claims

#### Scenario: Titles not read because the file was not opened
- **WHEN** a file is resolved from the content index
- **THEN** its `extraction` result and titles are both not attempted
  with reason `content-index-hit`

#### Scenario: Titles read for an operator's lookup
- **WHEN** the content index answered for a file, and an operator
  supplies an identifier for it and renames it from the record reached
- **THEN** its `resolved` event reports the `extraction` result as not
  attempted with reason `content-index-hit`, and the `titles` as `read`

