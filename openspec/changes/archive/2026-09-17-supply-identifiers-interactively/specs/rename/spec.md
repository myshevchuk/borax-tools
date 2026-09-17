## MODIFIED Requirements

### Requirement: An interactive run passes over already-named files
An interactive rename run SHALL, when the `rename.skip-named` setting is on, render nothing to the operator for a file that is already named: no resolution line, no outcome line and no question. The setting SHALL default to on, and `--no-skip-named` SHALL turn it off for a run.

A passed-over file SHALL still be reported in the event stream and the
run log exactly as when the setting is off, and the run's closing
summary SHALL state how many files were passed over.

With the setting off, an interactive run SHALL render already-named
files as a batch run does, and SHALL put the question an already-named
file is owed — keep its name, supply a different identifier, or quit —
so that a file named from the wrong record can be put right. The
setting SHALL NOT affect batch runs.

#### Scenario: New papers among renamed ones
- **WHEN** an interactive run reaches a directory holding three files
  that need renaming and two hundred already named
- **THEN** the operator sees the three questions and no line about the
  other two hundred, and the summary reports two hundred already named

#### Scenario: Showing named files
- **WHEN** the same run is started with `--no-skip-named`
- **THEN** each already-named file is shown as already named, and is
  asked about

#### Scenario: The run log keeps the full account
- **WHEN** an interactive run passes over an already-named file
- **THEN** its run log carries that file's resolution and its
  `already-named` event

## ADDED Requirements

### Requirement: An interactive run asks about files it could not settle
An interactive rename run SHALL put a question to the operator, in addition to the questions for proposed moves, for each file whose content hash is known and which: had no identifier found in it; had an identifier no service holds; could not be read as a PDF or is encrypted; or resolved to a record its claimed titles conflict with.

A file with no identifier, an unreadable body, or an identifier no
service holds SHALL be offered: supply an identifier, skip, or quit. A
file whose resolution failed without settling whether any service holds
its identifier — a service unreachable, rate-limited, or answering
unusably — SHALL additionally be offered a retry, first, and its
question SHALL say that the services did not answer rather than that
the identifier is unknown. A file with a
conflict whose proposal is a move SHALL additionally be offered renaming
to the proposed target despite the conflict, and that choice SHALL name
the target and SHALL NOT be the default. Every question about a proposed
move SHALL additionally offer supplying a different identifier.

When `rename.skip-named` is off, an already-named file SHALL be put to
the operator with the choices keep its name, supply a different
identifier, or quit. This is the one question an already-named file is
asked, and it is asked only when that setting is off; a run passing
over named files asks nothing about them.

A file reported as a duplicate, or whose content hash is unknown, SHALL
NOT be asked about.

Skipping any of these files SHALL leave it untouched and report it with
the reason a batch run would have given, not as `declined`. A file that
had a move on offer and was skipped after a supplied identifier led
nowhere SHALL be reported `declined`, since a move is what was
declined.

A supplied identifier that resolves SHALL leave the file in whatever
situation its new record puts it: a move to offer, a target taken, a
name that renders empty, or a file already named. A supplied
identifier that does not resolve, and an input the operator abandons,
SHALL leave the file exactly as it was, with the record and the choices
it already had.

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

#### Scenario: A supplied identifier that leads nowhere
- **WHEN** a file with a record and a move on offer is given another
  identifier, and no service holds it
- **THEN** what the services answered is shown, the file's original
  record is still what is on offer, and the operator may accept it,
  supply another, or skip

#### Scenario: Services that could not answer
- **WHEN** resolution failed because no service could be reached
- **THEN** the question says so and offers a retry before offering to
  supply an identifier

### Requirement: A candidate the operator abandoned leaves nothing behind
A record resolved from a supplied identifier and then abandoned — another identifier supplied, the file skipped, or the run quit — SHALL NOT be reported as the file's resolution, written to the content index, or cited. No sidecar SHALL be written from it and no entry SHALL be merged into the master bibliography from it.

Only the record a file's decision settled on SHALL reach any of those.
A file whose own resolution stands is cited from that record as it
would be in a batch run, whatever candidates were shown along the way.

#### Scenario: A wrong identifier is not written beside the file
- **WHEN** the operator supplies an identifier, sees a record for
  another paper, and supplies a different one instead
- **THEN** no sidecar and no bibliography entry is written from the
  abandoned record, and the file's own sidecar, when it is written,
  carries the record the operator accepted
