# record-model — delta for add-core-pipeline

## ADDED Requirements

### Requirement: The canonical record is a CSL-JSON superset
The internal bibliographic record SHALL use CSL-JSON field names and value
shapes for all standard bibliographic data, extended with: an identifier
set (DOI, arXiv ID, PMID, ISBN, each in normalized form), per-field
provenance (which source supplied the value), and a resolution confidence
value. Fields received from sources that have no CSL-JSON equivalent SHALL
be preserved in an extension area rather than dropped.

#### Scenario: Provenance survives merging
- **WHEN** a record's title comes from Crossref and its arXiv ID from the
  extraction pass
- **THEN** the stored record attributes the title to Crossref and the
  arXiv ID to extraction

### Requirement: Records round-trip through JSON losslessly
A record serialized to JSON and parsed back SHALL compare equal to the
original, including extensions, provenance, and confidence.

#### Scenario: Serialize and reparse
- **WHEN** any resolved record is serialized to JSON and parsed back
- **THEN** the parsed record equals the original

### Requirement: All planned document types are representable
The record model SHALL represent at minimum: journal article, preprint,
book, book chapter, thesis, report, patent, and standard — each mapping to
a defined CSL-JSON `type` value.

#### Scenario: Preprint with published version
- **WHEN** a record describes an arXiv preprint that also has a published
  DOI
- **THEN** the record holds both identifiers and its type distinguishes it
  from the published article

### Requirement: BibTeX emission is defined and deterministic
The model SHALL emit BibTeX/BibLaTeX entries with a defined type and field
mapping, escaping of special characters, and citation keys produced by the
template engine. Emitting the same record with the same configuration
SHALL yield byte-identical output.

#### Scenario: Special characters escaped
- **WHEN** a record's title contains "&", "%", and non-ASCII characters
- **THEN** the emitted BibTeX entry escapes or encodes them such that the
  file compiles under BibTeX and BibLaTeX

#### Scenario: Deterministic emission
- **WHEN** the same record is emitted twice with unchanged configuration
- **THEN** the two outputs are byte-identical
