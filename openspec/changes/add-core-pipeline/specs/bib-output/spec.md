# bib-output — delta for add-core-pipeline

## ADDED Requirements

### Requirement: Entries merge into a master .bib by identifier
When a master `.bib` file is configured, resolved entries SHALL be
appended to it, deduplicated by identifier (DOI or arXiv ID), not merely
by citation key. If an entry with the same identifier already exists, the
new entry SHALL be skipped by default; an explicit update flag SHALL
replace the existing entry in place.

#### Scenario: Duplicate identifier skipped
- **WHEN** a resolved DOI already appears in the master `.bib`
- **THEN** no new entry is appended and the event is reported as
  "already present"

#### Scenario: Explicit update
- **WHEN** the update flag is given for an entry whose DOI already exists
  in the master file
- **THEN** the existing entry is replaced in place

### Requirement: Untouched .bib content is preserved
Merging SHALL NOT reformat, reorder, or otherwise rewrite entries and
comments in the master file other than the entries it explicitly adds or
updates.

#### Scenario: Hand-edited file survives a merge
- **WHEN** entries are appended to a master file containing hand-formatted
  entries and comments
- **THEN** the pre-existing bytes are unchanged except for the appended
  content

### Requirement: Citation keys are unique within the master file
The merge SHALL append a deterministic letter suffix (`a`, `b`, `c`, …)
to a generated citation key that already exists in the master file for a
different identifier. Citation keys themselves are produced by the
template engine.

#### Scenario: Key clash between different works
- **WHEN** two different DOIs generate the key `smith2024`
- **THEN** the second entry is written with key `smith2024a`

### Requirement: Per-file sidecars carry the full record
When sidecar output is enabled, each processed file SHALL receive a
sidecar next to it, named after the file's final name, containing the
entry in BibTeX form and the full canonical record in JSON (including
provenance and identifiers), so no information is lost to the BibTeX
mapping.

#### Scenario: Sidecar follows the rename
- **WHEN** a file is renamed to `smith2024_borax.pdf` with sidecars
  enabled
- **THEN** its sidecar is created alongside as
  `smith2024_borax.pdf`-adjacent output named after the final filename
