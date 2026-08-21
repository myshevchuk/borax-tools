# bib-output — delta for settle-citation-keys

## MODIFIED Requirements

### Requirement: Citation keys are unique within the master file
The merge SHALL append a deterministic letter suffix (`a`, `b`, `c`, …)
to a generated citation key that already exists in the master file for a
different identifier.

Citation keys SHALL be produced by the template engine from a citation-key
template table of their own, independent of the filename templates, so
that changing how files are named does not change how works are cited.
The table has a mandatory default and optional per-entry-type overrides,
and the built-in default SHALL be `[auth:lower][year]`.

A rendered key SHALL be stripped of whitespace and of the characters a
BibTeX key cannot carry (`,`, `{`, `}`, `%`). A record whose key renders
empty, or renders nothing that survives stripping, SHALL be reported as
uncitable rather than given a fabricated key.

#### Scenario: Key clash between different works
- **WHEN** two different DOIs generate the key `smith2024`
- **THEN** the second entry is written with key `smith2024a`

#### Scenario: Default key shape
- **WHEN** a work by Smith published in 2024 is cited under the built-in
  configuration
- **THEN** its citation key is `smith2024`, whatever filename template
  is configured

#### Scenario: Filename template does not reach the key
- **WHEN** the filename template is changed and no citation-key template
  is set
- **THEN** the citation key is unchanged

#### Scenario: Per-entry-type key template
- **WHEN** a citation-key template is set for one entry type
- **THEN** records of that type render with it and every other type
  renders with the default

#### Scenario: Record too sparse to cite
- **WHEN** a record renders an empty citation key
- **THEN** it is reported as uncitable and no entry is written for it
