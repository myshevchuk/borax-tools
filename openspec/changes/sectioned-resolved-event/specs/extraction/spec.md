## ADDED Requirements

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
