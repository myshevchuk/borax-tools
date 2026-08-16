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

