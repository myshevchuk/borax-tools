## ADDED Requirements

### Requirement: An interactive run asks about files it could not settle
An interactive rename run SHALL put a question to the operator, in addition to the questions for proposed moves, for each file whose content hash is known and which: had no identifier found in it; had an identifier no service holds; could not be read as a PDF or is encrypted; or resolved to a record its claimed titles conflict with.

A file with no identifier, an unresolvable identifier, or an unreadable
body SHALL be offered: supply an identifier, skip, or quit. A file with a
conflict whose proposal is a move SHALL additionally be offered renaming
to the proposed target despite the conflict, and that choice SHALL name
the target and SHALL NOT be the default. Every question about a proposed
move SHALL additionally offer supplying a different identifier.

When `rename.skip-named` is off, an already-named file SHALL be put to
the operator with the choices keep its name, supply a different
identifier, or quit.

A file reported as a duplicate, or whose content hash is unknown, SHALL
NOT be asked about.

Skipping any of these files SHALL leave it untouched and report it with
the reason a batch run would have given, not as `declined`.

#### Scenario: Supplying an identifier for an unidentified file
- **WHEN** an interactive run finds no identifier in an author manuscript
  and the operator supplies the DOI of its published version and then
  answers rename
- **THEN** the file is renamed from the published record, and the run
  log records a `resolved` event with tier `supplied` and a `renamed`
  event for it

#### Scenario: Skipping an unidentified file
- **WHEN** the operator skips a file with no identifier
- **THEN** it keeps its name and is reported skipped with reason
  `no-identifier`

#### Scenario: A reference's DOI caught
- **WHEN** a proposal comes from a DOI that belongs to a paper the file
  cites, and the operator supplies the file's own DOI instead
- **THEN** a new proposal is made from the file's own record, and the
  name first proposed is not claimed

#### Scenario: Re-identifying a named file
- **WHEN** an interactive run with `--no-skip-named` reaches a file named
  from the wrong record, and the operator supplies the right identifier
  and answers rename
- **THEN** the file is renamed from the right record

### Requirement: A file's verdict follows the operator's decision
In an interactive run, the events reporting a file's resolution and fate SHALL be emitted once the operator's decisions about that file are made, and SHALL describe the outcome of those decisions: a file SHALL NOT be reported both skipped and renamed, and a record the operator abandoned for another identifier SHALL NOT be reported as the file's resolution.

A file renamed from a record accepted over a conflict SHALL be reported
with a `resolved` event carrying the conflict it overrode — the field,
both values and the similarity — followed by its `renamed` event.

#### Scenario: One report per file
- **WHEN** a file's extracted identifier conflicts, the operator supplies
  another identifier, and renames the file from its record
- **THEN** the stream carries one `resolved` event for the file, for the
  supplied identifier's record, followed by its `renamed` event, and no
  `skipped` event for it
