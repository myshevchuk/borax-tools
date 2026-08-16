# templates — delta for add-core-pipeline

## ADDED Requirements

### Requirement: Bracket template syntax
Templates SHALL consist of literal text and bracket tokens of the form
`[field]` or `[field:filter:filter...]`, where `field` names a record
field or derived accessor (e.g. `auth`, `authors2`, `year`, `title`,
`shorttitle3`, `journal`, `doi`, `sha1`) and filters apply left to right.
Literal text outside brackets is copied verbatim (subject to the
sanitization pass).

#### Scenario: Basic rendering
- **WHEN** the template `[auth:lower][year]_[shorttitle3:camel]` renders a
  record authored by Smith in 2024 titled "An Awesome Paper on Borax"
- **THEN** it produces `smith2024_AwesomePaperBorax`

### Requirement: Filter set with left-to-right chaining
The engine SHALL provide at minimum these filters: `lower`, `upper`,
`capitalize`, `titlecase`, `slug`, `abbr`, `truncN` (N a positive
integer), `transliterate`, and `regex("pattern","replacement")`. Filters
compose left to right; each receives the previous filter's output.

#### Scenario: Chained filters
- **WHEN** `[title:slug:trunc10]` renders a record titled "Borax: A Study"
- **THEN** the slug is computed first and then truncated to 10 characters

### Requirement: Alternatives select the first non-empty value
Within a bracket token, `||` SHALL separate alternatives; the token
evaluates to the first alternative producing non-empty output.

#### Scenario: Fallback identifier
- **WHEN** `[doi:slug || sha1:trunc8]` renders a record without a DOI
- **THEN** the token evaluates to the first 8 characters of the file hash

### Requirement: Per-entry-type template tables
Template configuration SHALL support one template per entry type with a
required `default` used when no type-specific template exists.

#### Scenario: Type-specific template wins
- **WHEN** templates define both `default` and `thesis` and a thesis
  record is rendered
- **THEN** the `thesis` template is used

### Requirement: Templates fail fast at load time
The engine SHALL report unknown fields, unknown filters, and template
syntax errors as configuration errors when the template is loaded —
before any file is processed — naming the template and the offending
token.

#### Scenario: Typo in a filter name
- **WHEN** a configured template contains `[title:lwoer]`
- **THEN** the run aborts before processing files with an error naming the
  unknown filter `lwoer`

### Requirement: Mandatory filename sanitization pass
After rendering, a non-optional sanitization pass SHALL: replace
characters invalid on any supported filesystem (including Windows), reject
or rewrite Windows reserved device names, enforce per-component and total
path length limits, and treat `/` produced by the template as a directory
separator. Sanitization applies per path component and cannot be disabled
by configuration.

#### Scenario: Windows-hostile title
- **WHEN** a rendered filename would be `CON: a study? of borax.pdf`
- **THEN** the sanitized result contains no reserved device name, no `:`,
  and no `?`, on every platform

### Requirement: Rendering is deterministic
The same record, template, and configuration SHALL always render the same
output.

#### Scenario: Repeated render
- **WHEN** a record is rendered twice with unchanged template and
  configuration
- **THEN** both outputs are identical
