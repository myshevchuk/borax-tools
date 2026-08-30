## ADDED Requirements

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

## MODIFIED Requirements

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

### Requirement: Templates fail fast at load time
The engine SHALL report unknown fields, unknown filters, unknown lookup
tables, and template syntax errors as configuration errors when the
template is loaded — before any file is processed — naming the template
and the offending token.

A `lookup` naming a table no configuration declared is of this kind: the
set of declared tables is known when templates are compiled, so a
template that could never resolve is refused then rather than rendering
empty on every file.

#### Scenario: Typo in a filter name
- **WHEN** a configured template contains `[title:lwoer]`
- **THEN** the run aborts before processing files with an error naming the
  unknown filter `lwoer`

#### Scenario: Lookup names no declared table
- **WHEN** a configured template contains `[journal:lookup("jcode")]`
  and no `jcode` table is declared
- **THEN** the run aborts before processing files with an error naming
  the template and the unknown table `jcode`
