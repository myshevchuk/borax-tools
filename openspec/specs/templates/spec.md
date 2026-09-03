# templates Specification

## Purpose
TBD - created by archiving change add-core-pipeline. Update Purpose after archive.
## Requirements
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

### Requirement: Publication fields are reachable from a template
The field vocabulary SHALL include `volume`, `issue`, `pages`, `firstpage` and `publisher`, each rendering the corresponding record value and, like every field, rendering the empty string when the record lacks it.

`pages` renders the page value as the record holds it. `firstpage` renders the part of that value before the first `-`, `–` or `—`, trimmed of surrounding whitespace, and renders the whole value when it holds none — so a page range yields its first page and an article number passes through intact.

#### Scenario: Volume and first page render
- **WHEN** `[year]-[volume]-[firstpage]` renders a 2024 record in volume
  146 with pages `1234-1245`
- **THEN** it produces `2024-146-1234`

#### Scenario: An article number is not a range
- **WHEN** `[firstpage]` renders a record whose page value is `e0123456`
- **THEN** it produces `e0123456`

#### Scenario: An absent volume renders empty
- **WHEN** `[volume]` renders a record with no volume
- **THEN** the token contributes nothing and the run continues

### Requirement: Filter set with left-to-right chaining
The engine SHALL provide at minimum these filters: `lower`, `upper`,
`capitalize`, `titlecase`, `slug`, `abbr`, `truncN` (N a positive
integer), `transliterate`, `regex("pattern","replacement")`,
`lookup("table")`, `prefix("text")`, and `suffix("text")`. Filters
compose left to right; each receives the previous filter's output.

`lookup` consults a table declared in configuration; its semantics,
including how its input is matched and what a miss does, belong to the
`external-tables` capability.

#### Scenario: Chained filters
- **WHEN** `[title:slug:trunc10]` renders a record titled "Borax: A Study"
- **THEN** the slug is computed first and then truncated to 10 characters

#### Scenario: Lookup composes with other filters
- **WHEN** `[journal:lookup("jcode"):lower]` renders a record whose
  container title the table maps to `JACS`
- **THEN** it produces `jacs`

### Requirement: Affix filters leave the empty string alone
The `prefix("<text>")` and `suffix("<text>")` filters SHALL return their input with the given text placed before or after it, and SHALL return the empty string unchanged.

Being the identity on the empty string is the whole point: it lets a separator belong to the optional segment it separates, so a token that renders nothing takes its separator with it, without the grammar acquiring conditionals.

#### Scenario: An optional segment carries its separator
- **WHEN** `[year]-[journal:abbr][volume:prefix("-")]-[firstpage]`
  renders a record in volume 146
- **THEN** the volume contributes `-146`

#### Scenario: The same template with no volume
- **WHEN** that template renders a record with no volume
- **THEN** no stray separator appears where the volume would have been

#### Scenario: Suffix on a present value
- **WHEN** `[auth:suffix("-")]` renders a record whose first author is
  Smith
- **THEN** it produces `Smith-`

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
The engine SHALL report unknown fields, unknown filters, unknown lookup
tables, and template syntax errors as configuration errors when the
template is loaded — before any file is processed — naming the template
and the offending token.

A `lookup` naming a table no configuration declared is of this kind: the
set of declared tables is known when templates are compiled, so a
template that could never resolve is refused then rather than rendering
empty on every file.

A run SHALL compile the template tables it renders from and no others. A
command that renders no filename therefore does not compile the filename
templates, and a filename template that will not compile cannot end it —
a run must not be refused over a value it was never going to render. A
command that renders neither, such as one that only reports the
effective configuration, compiles neither and cannot be ended by a
template at all; reporting a configuration is most useful on the
configurations other commands refuse.

Lookup tables are loaded regardless, since loading one is what validates
the `lookup` tokens in whichever templates are compiled, and a table
that will not load is a fault in the configuration rather than in a
template.

#### Scenario: Typo in a filter name
- **WHEN** a configured template contains `[title:lwoer]`
- **THEN** the run aborts before processing files with an error naming the
  unknown filter `lwoer`

#### Scenario: Lookup names no declared table
- **WHEN** a configured template contains `[journal:lookup("jcode")]`
  and no `jcode` table is declared
- **THEN** the run aborts before processing files with an error naming
  the template and the unknown table `jcode`

#### Scenario: A filename template stops only the runs that render one
- **WHEN** `templates.default` will not compile and `borax bib` runs
- **THEN** the bibliography run proceeds, and `borax rename` in the same
  directory still aborts before processing files naming that template

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

