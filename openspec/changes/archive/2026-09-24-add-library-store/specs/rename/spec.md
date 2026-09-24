## MODIFIED Requirements

### Requirement: An interactive run asks before each move
An interactive run SHALL put a question to the operator for each file whose plan is a move to a free target, before the file is moved, cited or admitted. The question SHALL name the file and the target it would move to, including any collision suffix, and SHALL offer at least: rename, skip, and quit.

A rename answer SHALL carry out the move as an applying batch run would
carry it out, and report it the same way. A skip answer SHALL leave the
file untouched and report it skipped with reason `declined`.

A proposed target SHALL be claimed only when the operator accepts it. A
declined proposal SHALL leave the run's plan as though that file had
proposed nothing, so no later file is suffixed on account of a move that
did not happen.

A file whose plan is not a move — already named, blocked by a collision
under the skip policy, or unnameable — and a file that did not resolve
SHALL be reported as a batch run reports it, without a question, unless
another requirement gives it one.

Files SHALL be asked about in the order a batch run would report them,
and each file's resolution, question, fate and sidecar SHALL remain
adjacent.

#### Scenario: Accepting a proposal
- **WHEN** the operator answers rename to the question for `a.pdf →
  smith2024.pdf`
- **THEN** the file is moved, a `renamed` event names both paths and the
  content hash, and an artifact record names its new library-relative
  path and the item it belongs to, as an applied rename would

#### Scenario: Declining a proposal
- **WHEN** the operator answers skip to the question for `a.pdf`
- **THEN** `a.pdf` keeps its name and is reported skipped with reason
  `declined`

#### Scenario: A declined name stays free
- **WHEN** `a.pdf` and `b.pdf` in one directory both render
  `smith2024.pdf`, and the operator declines `a.pdf`
- **THEN** the question for `b.pdf` proposes `smith2024.pdf` without a
  suffix

#### Scenario: A file with nothing to decide
- **WHEN** an interactive run reaches a file that already carries the
  name its record implies
- **THEN** it is reported as already named and no question is put

### Requirement: An interactive run asks about files it could not settle
An interactive rename run SHALL put a question to the operator, in addition to the questions for proposed moves, for each file whose content hash is known and which: had no identifier found in it; had an identifier no service holds; could not be read as a PDF or is encrypted; resolved to a record its claimed titles conflict with; or resolved to an identifier an item already in the library carries.

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

A file whose own resolution lands on an identifier an item already
carries SHALL be
offered: file it as another artifact of that item, skip, or quit. The
question SHALL name the item and a path already recorded against it
that still holds a file, so that the operator is deciding about a work
they can see they hold, and filing SHALL NOT be the default. An item
with no such path is not a work duplicate at all and SHALL NOT produce
this question: there is no file to compare against, and the incoming
one is that item's first artifact. Accepting SHALL return the file to
planning, where the ordinary question about its move is put; only then
is it moved and recorded. Declining SHALL report it with the
work-duplicate reason a batch run would have given. This is the one
question borax asks about a duplicate, and only an operator can answer
it: a second artifact of a work and an unwanted second copy of it are
one thing in the bytes and two in the intention.

A record the operator reached rather than one the run resolved — an
identifier they supplied, a lookup they retried, a record they accepted
over a conflict — SHALL be checked against the library the same way and
SHALL NOT produce that question. Where it lands on a work the library
already holds a file of, the run SHALL say so, naming the same item and
recorded path the question would have named, and SHALL put the ordinary
question about the file's move again; the answer given after it stands,
and a move carried out then admits the file as another artifact of that
item. The collision SHALL be stated once, so that a second answer is
never asked for twice and the move is never unreachable. Supplying an
identifier is the operator saying what the file is, which is the
statement the filing question exists to obtain, so what is left is not
a decision to ask for but a fact they have not been told.

#### Scenario: A supplied identifier lands on a work already held
- **WHEN** an interactive run's operator supplies the DOI of a work the
  library already holds a file of, and answers rename
- **THEN** the run reports the item and the recorded path it collided
  with, puts the question about the move again, and a second rename
  answer moves the file and records it as another artifact of that
  item, with no second item minted

When `rename.skip-named` is off, an already-named file SHALL be put to
the operator with the choices keep its name, supply a different
identifier, or quit. This is the one question an already-named file is
asked, and it is asked only when that setting is off; a run passing
over named files asks nothing about them.

A file reported as a content duplicate, or whose content hash is
unknown, SHALL NOT be asked about. The same bytes are already in the
library, so there is nothing an operator could add: the question above
is asked about a work the library holds, never about a file it holds.

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
- **THEN** the file is renamed from the right record, and if it has an
  artifact record that record is re-linked to the item for the supplied
  record, so its name and what the library says it is are corrected
  together

#### Scenario: Filing a second artifact of a work already held
- **WHEN** an interactive run reaches a scan of a paper whose published
  PDF the library already holds under the same DOI, and the operator
  answers that it is another artifact of that item
- **THEN** the question names the item and the recorded path of the
  artifact already linked to it, the ordinary move question follows, and
  answering rename files the scan and records it against that same item

#### Scenario: Declining a second copy
- **WHEN** the operator answers skip to that question
- **THEN** the file keeps its name and is reported with the
  work-duplicate reason a batch run would have given, not as `declined`

#### Scenario: A content duplicate is still never asked about
- **WHEN** an interactive run reaches a file whose hash is already in an
  artifact record's history
- **THEN** it is reported a content duplicate and no question is put

### Requirement: A file already carrying its name is not a skip
A file whose current name is the name its record implies under the configuration in force, or whose target is occupied by a byte-identical file, SHALL be reported as an `already-named` outcome of its own, and SHALL NOT be reported or counted as skipped.

`run-finished` SHALL count already-named files in a total of their own.
Already-named files SHALL NOT make a run exit with the partial-success
code: a run over files that are all already named exits 0.

Whether a file is already named SHALL be decided by rendering its record
through the templates in force for its directory, never by whether the
library holds an artifact record for it, so a file named under an
earlier template is not already named under a changed one.

An applying run SHALL write an artifact record for an already-named file
inside the library, as it does for one it moved: the run holds the
record, agrees with the name, and a library already in good order would
otherwise never be recorded at all. Nothing is moved, and a preview
writes no record.

#### Scenario: Re-run over a folder in order
- **WHEN** `borax rename --batch` runs over a directory in which every
  file already carries the name its record implies
- **THEN** each file is reported `already-named`, `run-finished` counts
  them as named and none as skipped, and the exit code is 0

#### Scenario: Template changed since the last run
- **WHEN** a file was renamed under one filename template and the
  configuration now holds a different one
- **THEN** the file is not already named, and a rename to the new
  template's name is planned for it

#### Scenario: An already-named file is recorded
- **WHEN** `borax rename --apply` runs over a library whose files all
  already carry the names their records imply and none of which has an
  artifact record
- **THEN** nothing is moved, every file is reported `already-named`, the
  exit code is 0, and each file now has an artifact record naming an
  item
