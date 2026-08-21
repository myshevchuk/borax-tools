# cli — delta for settle-citation-keys

## ADDED Requirements

### Requirement: Citation-key templates are configured separately
The configuration SHALL carry a `citation-keys` table beside
`templates`, with the same shape and the same open-ended keys: a
`default` that every entry type falls back to, plus optional
per-entry-type overrides. A key naming no entry type, and a template
that will not compile, SHALL be configuration errors reported before any
file is processed — the same treatment `templates` receives, for the
same reason: a broken template is wrong for every file in the batch.
`borax config` SHALL report the effective citation-key templates and the
origin of each.

#### Scenario: Citation-key override reported
- **WHEN** a configuration file sets `citation-keys.default`
- **THEN** `borax config` reports that value with the file as its origin

#### Scenario: Uncompilable citation-key template
- **WHEN** `citation-keys.default` will not compile
- **THEN** the run aborts as a configuration error naming the key,
  before any file is processed

#### Scenario: Citation-key key names no entry type
- **WHEN** `citation-keys` carries a key matching no entry type
- **THEN** the run aborts as a configuration error naming the key
